//! The SQL execution engine.
//!
//! Security model (enforced by PostgreSQL, not by inspecting SQL text):
//! * Each execution uses a **dedicated connection** that is closed afterwards,
//!   so session state (`SET`, temp tables, prepared statements) never leaks
//!   between executions or users.
//! * Read-only connections run inside `BEGIN READ ONLY` (plus the
//!   `default_transaction_read_only` startup option) and are rolled back.
//! * Statements are sent with the **extended protocol**, which only accepts a
//!   single statement, so `SELECT 1; COMMIT; DROP ...` is rejected by the
//!   server instead of escaping the read-only transaction.
//! * `statement_timeout` bounds runtime; a row limit and a byte budget bound
//!   memory; a per-user concurrency limit bounds resource usage.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures_util::TryStreamExt;
use serde_json::Value;
use sqlx::{Column, Connection, Either, Executor, PgConnection, Row, Statement};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::query::{ColumnMeta, ExecuteRequest, ExplainRequest, ExplainResult, QueryResult},
    services::{
        auth::AuthUser,
        connections::ConnectionService,
        target_db::{self, TargetSpec},
        value_decode::{decode_cell, type_label},
    },
    state::AppState,
    utils::sql_text,
};

/// Approximate upper bound on the serialized size of a result set.
const MAX_RESULT_BYTES: usize = 32 * 1024 * 1024;
const SQLSTATE_QUERY_CANCELED: &str = "57014";

// ---------------------------------------------------------------------------
// Running-execution registry (cancellation + concurrency limits)
// ---------------------------------------------------------------------------

struct Running {
    user_id: Uuid,
    connection_id: Uuid,
    /// Backend process id on the target server (0 until connected).
    pid: i32,
    cancel_requested: bool,
}

#[derive(Default)]
pub struct ExecutionRegistry {
    inner: Mutex<HashMap<Uuid, Running>>,
}

/// Removes the registry entry when the execution finishes, however it ends.
pub struct ExecutionGuard {
    registry: Arc<ExecutionRegistry>,
    id: Uuid,
}

impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.id);
    }
}

impl ExecutionRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Running>> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn register(
        self: &Arc<Self>,
        id: Uuid,
        user_id: Uuid,
        connection_id: Uuid,
        max_per_user: usize,
    ) -> AppResult<ExecutionGuard> {
        let mut map = self.lock();
        if map.contains_key(&id) {
            return Err(AppError::Conflict(
                "an execution with this id is already running".into(),
            ));
        }
        let running = map.values().filter(|r| r.user_id == user_id).count();
        if running >= max_per_user {
            return Err(AppError::RateLimited {
                retry_after_secs: 1,
            });
        }
        map.insert(
            id,
            Running {
                user_id,
                connection_id,
                pid: 0,
                cancel_requested: false,
            },
        );
        Ok(ExecutionGuard {
            registry: Arc::clone(self),
            id,
        })
    }

    fn set_pid(&self, id: Uuid, pid: i32) {
        if let Some(r) = self.lock().get_mut(&id) {
            r.pid = pid;
        }
    }

    fn was_cancelled(&self, id: Uuid) -> bool {
        self.lock().get(&id).is_some_and(|r| r.cancel_requested)
    }

    /// Flags the execution as cancelled and returns its connection and pid.
    /// Only the owning user can cancel; others get `None` (→ 404).
    fn request_cancel(&self, id: Uuid, user_id: Uuid) -> Option<(Uuid, i32)> {
        let mut map = self.lock();
        let r = map.get_mut(&id).filter(|r| r.user_id == user_id)?;
        r.cancel_requested = true;
        Some((r.connection_id, r.pid))
    }

    pub fn running_for_user(&self, user_id: Uuid) -> usize {
        self.lock()
            .values()
            .filter(|r| r.user_id == user_id)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

struct Outcome {
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<Value>>,
    rows_affected: Option<u64>,
    truncated: bool,
    /// Reading stopped before the statement completed (connection must be dropped).
    stopped_early: bool,
}

fn approx_size(v: &Value) -> usize {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => 8,
        Value::String(s) => s.len() + 2,
        other => other.to_string().len(),
    }
}

async fn run_statement(
    conn: &mut PgConnection,
    sql: &str,
    max_rows: usize,
    drain: bool,
) -> Result<Outcome, sqlx::Error> {
    // Prepare first so column metadata is available even for empty results;
    // this is also where multi-statement input is rejected.
    let statement = (&mut *conn).prepare(sql).await?;
    let columns: Vec<ColumnMeta> = statement
        .columns()
        .iter()
        .map(|c| ColumnMeta {
            name: c.name().to_string(),
            type_name: type_label(c.type_info()),
        })
        .collect();

    let mut rows = Vec::new();
    let mut bytes = 0usize;
    let mut rows_affected = None;
    let mut truncated = false;
    let mut stopped_early = false;

    let mut stream = (&mut *conn).fetch_many(statement.query());
    while let Some(item) = stream.try_next().await? {
        match item {
            Either::Left(done) => rows_affected = Some(done.rows_affected()),
            Either::Right(row) => {
                if rows.len() < max_rows && bytes < MAX_RESULT_BYTES {
                    let values: Vec<Value> = (0..row.len()).map(|i| decode_cell(&row, i)).collect();
                    bytes += values.iter().map(approx_size).sum::<usize>();
                    rows.push(values);
                } else {
                    truncated = true;
                    if !drain {
                        stopped_early = true;
                        break;
                    }
                }
            }
        }
    }
    drop(stream);
    Ok(Outcome {
        columns,
        rows,
        rows_affected,
        truncated,
        stopped_early,
    })
}

fn clamp_timeout(state: &AppState, requested: Option<u32>) -> Duration {
    let max = state.config.query_max_timeout;
    requested
        .map(|s| Duration::from_secs(s.into()))
        .unwrap_or(state.config.query_default_timeout)
        .min(max)
}

async fn begin(conn: &mut PgConnection, read_only: bool, timeout: Duration) -> AppResult<()> {
    let begin = if read_only {
        "BEGIN READ ONLY"
    } else {
        "BEGIN"
    };
    conn.execute(begin).await.map_err(target_db::query_error)?;
    target_db::set_local_timeouts(conn, timeout).await
}

async fn close(conn: PgConnection) {
    let _ = tokio::time::timeout(Duration::from_secs(2), conn.close()).await;
}

/// Opens a connection for an owned connection record and registers the backend pid.
async fn open_registered(
    state: &AppState,
    user: &AuthUser,
    connection_id: Uuid,
    execution_id: Uuid,
    force_read_only: bool,
) -> AppResult<(PgConnection, bool, ExecutionGuard)> {
    state.query_limiter.check(&user.user_id.to_string())?;
    let service = ConnectionService::new(state);
    let record = service.get_record(user.user_id, connection_id).await?;
    let guard = state.executions.register(
        execution_id,
        user.user_id,
        record.id,
        state.config.max_concurrent_queries_per_user,
    )?;
    let secret = service.decrypt_secret(&record)?;
    let mut spec = TargetSpec::from_record(&record, &secret);
    let read_only = force_read_only || record.read_only;
    spec.read_only = read_only;
    let mut conn = target_db::connect(&state.config, &spec).await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut conn)
        .await
        .map_err(target_db::query_error)?;
    state.executions.set_pid(execution_id, pid);
    if state.executions.was_cancelled(execution_id) {
        close(conn).await;
        return Err(AppError::QueryCancelled {
            execution_time_ms: 0,
        });
    }
    Ok((conn, read_only, guard))
}

fn map_execution_error(
    state: &AppState,
    execution_id: Uuid,
    err: sqlx::Error,
    timeout: Duration,
    elapsed_ms: u64,
) -> AppError {
    if let sqlx::Error::Database(db) = &err {
        if db.code().as_deref() == Some(SQLSTATE_QUERY_CANCELED) {
            return if state.executions.was_cancelled(execution_id) {
                AppError::QueryCancelled {
                    execution_time_ms: elapsed_ms,
                }
            } else {
                AppError::QueryTimeout {
                    timeout_secs: timeout.as_secs(),
                    execution_time_ms: elapsed_ms,
                }
            };
        }
    }
    match target_db::query_error(err) {
        AppError::Sql(mut info) => {
            info.execution_time_ms = Some(elapsed_ms);
            AppError::Sql(info)
        }
        other => other,
    }
}

pub async fn execute(
    state: &AppState,
    user: &AuthUser,
    req: &ExecuteRequest,
) -> AppResult<QueryResult> {
    let execution_id = req.execution_id.unwrap_or_else(Uuid::new_v4);
    let timeout = clamp_timeout(state, req.timeout_secs);
    let max_rows = req
        .max_rows
        .map(|n| n as usize)
        .unwrap_or(state.config.query_default_max_rows)
        .min(state.config.query_max_rows);

    let (mut conn, read_only, _guard) =
        open_registered(state, user, req.connection_id, execution_id, false).await?;
    begin(&mut conn, read_only, timeout).await?;

    let started = Instant::now();
    let outcome = run_statement(&mut conn, &req.query, max_rows, !read_only).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let outcome = match outcome {
        Ok(o) => o,
        Err(err) => {
            // Dropping the connection aborts the transaction server-side.
            drop(conn);
            return Err(map_execution_error(
                state,
                execution_id,
                err,
                timeout,
                elapsed_ms,
            ));
        }
    };

    if outcome.stopped_early {
        // Unread rows are still in flight; closing the socket is cheaper than
        // draining them and implicitly rolls back the read-only transaction.
        drop(conn);
    } else {
        let end = if read_only { "ROLLBACK" } else { "COMMIT" };
        if let Err(err) = conn.execute(end).await {
            drop(conn);
            return Err(map_execution_error(
                state,
                execution_id,
                err,
                timeout,
                elapsed_ms,
            ));
        }
        close(conn).await;
    }

    let command = sql_text::leading_keyword(&req.query);
    let rows_affected = if outcome.columns.is_empty() || sql_text::is_dml(command.as_deref()) {
        outcome.rows_affected
    } else {
        None
    };
    Ok(QueryResult {
        status: "success",
        row_count: outcome.rows.len(),
        columns: outcome.columns,
        rows: outcome.rows,
        rows_affected,
        truncated: outcome.truncated,
        execution_time_ms: elapsed_ms,
        command,
        execution_id,
        history_id: None,
    })
}

/// `EXPLAIN` (optionally `ANALYZE`). Always runs in a read-only transaction
/// that is rolled back, so ANALYZE cannot modify data even on read-write
/// connections — PostgreSQL rejects writes inside the transaction.
pub async fn explain(
    state: &AppState,
    user: &AuthUser,
    req: &ExplainRequest,
) -> AppResult<ExplainResult> {
    let execution_id = req.execution_id.unwrap_or_else(Uuid::new_v4);
    let timeout = clamp_timeout(state, req.timeout_secs);
    let (mut conn, _, _guard) =
        open_registered(state, user, req.connection_id, execution_id, true).await?;
    begin(&mut conn, true, timeout).await?;

    let options = if req.analyze {
        "ANALYZE, BUFFERS, VERBOSE, FORMAT JSON"
    } else {
        "VERBOSE, FORMAT JSON"
    };
    // The user's statement is appended verbatim: it is arbitrary SQL by
    // design. Safety comes from the single-statement protocol and the
    // read-only transaction, not from the text.
    let sql = format!("EXPLAIN ({options})\n{}", req.query);
    let started = Instant::now();
    let result = sqlx::query_scalar::<_, Value>(&sql)
        .fetch_one(&mut conn)
        .await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let raw = match result {
        Ok(v) => v,
        Err(err) => {
            drop(conn);
            return Err(map_execution_error(
                state,
                execution_id,
                err,
                timeout,
                elapsed_ms,
            ));
        }
    };
    let _ = conn.execute("ROLLBACK").await;
    close(conn).await;

    let root = raw.get(0).cloned().unwrap_or(Value::Null);
    Ok(ExplainResult {
        plan: root.get("Plan").cloned().unwrap_or(Value::Null),
        planning_time_ms: root.get("Planning Time").and_then(Value::as_f64),
        execution_time_ms: root.get("Execution Time").and_then(Value::as_f64),
        analyzed: req.analyze,
        raw,
    })
}

/// Cancels a running execution owned by `user` via `pg_cancel_backend`.
pub async fn cancel(state: &AppState, user: &AuthUser, execution_id: Uuid) -> AppResult<bool> {
    let (connection_id, pid) = state
        .executions
        .request_cancel(execution_id, user.user_id)
        .ok_or(AppError::NotFound("execution"))?;
    if pid == 0 {
        // Still connecting; the execution checks the flag before running.
        return Ok(true);
    }
    let service = ConnectionService::new(state);
    let record = service.get_record(user.user_id, connection_id).await?;
    let secret = service.decrypt_secret(&record)?;
    let mut spec = TargetSpec::from_record(&record, &secret);
    spec.read_only = true;
    let mut conn = target_db::connect(&state.config, &spec).await?;
    let cancelled: bool = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
        .bind(pid)
        .fetch_one(&mut conn)
        .await
        .map_err(target_db::query_error)?;
    close(conn).await;
    tracing::info!(%execution_id, user_id = %user.user_id, cancelled, "query cancellation requested");
    Ok(cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_enforces_ownership_and_concurrency() {
        let registry = Arc::new(ExecutionRegistry::default());
        let (alice, bob, conn) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let a1 = Uuid::new_v4();
        let g1 = registry.register(a1, alice, conn, 2).unwrap();
        let _g2 = registry.register(Uuid::new_v4(), alice, conn, 2).unwrap();
        assert!(matches!(
            registry.register(Uuid::new_v4(), alice, conn, 2),
            Err(AppError::RateLimited { .. })
        ));
        assert!(matches!(
            registry.register(a1, bob, conn, 2),
            Err(AppError::Conflict(_))
        ));

        registry.set_pid(a1, 42);
        assert_eq!(
            registry.request_cancel(a1, bob),
            None,
            "other users cannot cancel"
        );
        assert_eq!(registry.request_cancel(a1, alice), Some((conn, 42)));
        assert!(registry.was_cancelled(a1));

        drop(g1);
        assert_eq!(registry.running_for_user(alice), 1);
        assert!(!registry.was_cancelled(a1));
    }
}

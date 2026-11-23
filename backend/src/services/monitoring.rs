//! Live monitoring of a target server: sessions, locks, sizes and
//! `pg_stat_statements`, plus audited cancel/terminate.
//!
//! Every view opens a dedicated read-only connection with a 5 second
//! statement timeout. Nothing polls in the background: views refresh only
//! while open in the browser, and refreshes are rate limited per user,
//! connection and view.

use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};

use sqlx::{postgres::PgDatabaseError, PgConnection};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::{
        connection::ConnectionRecord,
        library::HistoryKind,
        monitoring::{
            blocking_tree, ActivityResponse, IndexSize, LockInfo, LocksResponse, Privileges,
            SchemaSize, Session, Signal, SignalResponse, SizesResponse, TableSize,
            TopQueriesResponse, TopQueriesSort, TopQuery,
        },
        workspaces::Access,
    },
    services::{
        connections::ConnectionService,
        data_studio::check_confirmation,
        library::LibraryService,
        schema::{close, USER_SCHEMAS},
        sql_builder::quote_ident,
        target_db::{self, TargetSpec},
    },
    state::AppState,
};

const MONITOR_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SESSIONS: i64 = 500;
const MAX_LOCKS: i64 = 1000;
const MAX_SIZE_ROWS: i64 = 500;
const MAX_INDEX_ROWS: i64 = 200;
const MAX_TOP_QUERIES: i64 = 100;

const READ_ONLY_CONNECTION: &str = "This connection is read-only. Enable read-write mode in the \
     connection settings to cancel or terminate sessions.";

#[derive(Clone, Copy)]
enum View {
    Activity,
    Locks,
    Sizes,
    TopQueries,
}

impl View {
    fn key(self) -> &'static str {
        match self {
            View::Activity => "activity",
            View::Locks => "locks",
            View::Sizes => "sizes",
            View::TopQueries => "top-queries",
        }
    }
}

const PRIVILEGES_SQL: &str = "SELECT current_user::text AS role, \
       COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), false) AS superuser, \
       pg_has_role(current_user, 'pg_read_all_stats', 'MEMBER') AS read_all_stats, \
       pg_has_role(current_user, 'pg_signal_backend', 'MEMBER') AS signal_backend";

/// Sessions of the current database. Without privileges PostgreSQL shows
/// `<insufficient privilege>` instead of other roles' statements.
const ACTIVITY_SQL: &str = "SELECT a.pid, a.usename::text AS user, a.application_name AS application, \
       host(a.client_addr) AS client, a.backend_type, a.state, a.wait_event_type, a.wait_event, \
       a.backend_start, a.xact_start, a.query_start, \
       (EXTRACT(EPOCH FROM clock_timestamp() - a.query_start) * 1000)::float8 AS query_ms, \
       (EXTRACT(EPOCH FROM clock_timestamp() - a.xact_start) * 1000)::float8 AS xact_ms, \
       CASE WHEN a.query = '<insufficient privilege>' THEN NULL ELSE left(a.query, 2000) END AS query, \
       COALESCE(a.query = '<insufficient privilege>', false) AS query_hidden, \
       pg_blocking_pids(a.pid) AS blocked_by, \
       a.pid = pg_backend_pid() AS is_self \
     FROM pg_stat_activity a \
     WHERE a.datname = current_database() \
     ORDER BY (a.state = 'active') DESC NULLS LAST, a.query_start NULLS LAST, a.pid \
     LIMIT $1";

pub struct MonitoringService<'a> {
    state: &'a AppState,
}

impl<'a> MonitoringService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    async fn connect(&self, record: &ConnectionRecord, read_only: bool) -> AppResult<PgConnection> {
        self.state
            .connect_limiter
            .check(&record.user_id.to_string())?;
        let service = ConnectionService::new(self.state);
        let secret = service.decrypt_secret(record)?;
        let mut spec = TargetSpec::from_record(record, &secret);
        spec.read_only = read_only || record.read_only;
        let mut conn = target_db::connect(&self.state.config, &spec).await?;
        target_db::set_statement_timeout(&mut conn, MONITOR_TIMEOUT).await?;
        Ok(conn)
    }

    /// Opens a read-only monitoring connection after the refresh limit.
    async fn open(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        view: View,
    ) -> AppResult<(ConnectionRecord, PgConnection)> {
        let record = ConnectionService::new(self.state)
            .get_record(user_id, connection_id)
            .await?;
        self.state
            .monitor_limiter
            .check(&format!("{user_id}:{connection_id}:{}", view.key()))?;
        let conn = self.connect(&record, true).await?;
        Ok((record, conn))
    }

    pub async fn activity(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
    ) -> AppResult<ActivityResponse> {
        let (record, mut conn) = self.open(user_id, connection_id, View::Activity).await?;
        let result = async {
            let privileges = privileges(&mut conn, &record).await?;
            let sessions = sessions(&mut conn).await?;
            Ok(ActivityResponse {
                privileges,
                sessions,
                server_time: chrono::Utc::now(),
            })
        }
        .await;
        close(conn).await;
        result
    }

    pub async fn locks(&self, user_id: Uuid, connection_id: Uuid) -> AppResult<LocksResponse> {
        let (record, mut conn) = self.open(user_id, connection_id, View::Locks).await?;
        let result = async {
            let privileges = privileges(&mut conn, &record).await?;
            let all = sessions(&mut conn).await?;
            let locks = sqlx::query_as::<_, LockInfo>(
                "SELECT l.pid, l.locktype, l.mode, l.granted, \
                        CASE WHEN l.relation IS NOT NULL THEN l.relation::regclass::text END AS relation \
                 FROM pg_locks l JOIN pg_stat_activity a ON a.pid = l.pid \
                 WHERE a.datname = current_database() AND l.pid <> pg_backend_pid() \
                 ORDER BY l.granted, l.pid, l.locktype \
                 LIMIT $1",
            )
            .bind(MAX_LOCKS)
            .fetch_all(&mut conn)
            .await
            .map_err(target_db::query_error)?;

            let pairs: Vec<(i32, Vec<i32>)> =
                all.iter().map(|s| (s.pid, s.blocked_by.clone())).collect();
            let tree = blocking_tree(&pairs);
            let mut involved = HashSet::new();
            for s in &all {
                if !s.blocked_by.is_empty() {
                    involved.insert(s.pid);
                    involved.extend(s.blocked_by.iter().copied());
                }
            }
            let waiting = all.iter().filter(|s| !s.blocked_by.is_empty()).count();
            let sessions: BTreeMap<String, Session> = all
                .into_iter()
                .filter(|s| involved.contains(&s.pid))
                .map(|s| (s.pid.to_string(), s))
                .collect();
            let locks = locks
                .into_iter()
                .filter(|l| involved.contains(&l.pid) || !l.granted)
                .collect();
            Ok(LocksResponse {
                privileges,
                tree,
                sessions,
                locks,
                waiting,
            })
        }
        .await;
        close(conn).await;
        result
    }

    pub async fn sizes(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        schema: Option<&str>,
    ) -> AppResult<SizesResponse> {
        let (_, mut conn) = self.open(user_id, connection_id, View::Sizes).await?;
        let result = load_sizes(&mut conn, schema).await;
        close(conn).await;
        result
    }

    pub async fn top_queries(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        sort: TopQueriesSort,
    ) -> AppResult<TopQueriesResponse> {
        let (record, mut conn) = self.open(user_id, connection_id, View::TopQueries).await?;
        let result = async {
            let privileges = privileges(&mut conn, &record).await?;
            let (queries, reason) = match load_top_queries(&mut conn, sort).await? {
                Ok(q) => (q, None),
                Err(reason) => (Vec::new(), Some(reason)),
            };
            Ok(TopQueriesResponse {
                available: reason.is_none(),
                reason,
                privileges,
                queries,
            })
        }
        .await;
        close(conn).await;
        result
    }

    /// Cancels the running statement of, or terminates, another session of
    /// the same database. Requires a read-write connection and the typed
    /// confirmation for staging/production; every attempt is recorded in
    /// the user's history.
    pub async fn signal(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        pid: i32,
        signal: Signal,
        confirm: Option<&str>,
    ) -> AppResult<SignalResponse> {
        let record = ConnectionService::new(self.state)
            .require(user_id, connection_id, Access::Write)
            .await?
            .record;
        if record.read_only {
            return Err(AppError::Forbidden(READ_ONLY_CONNECTION.into()));
        }
        check_confirmation(&record, confirm)?;
        let mut conn = self.connect(&record, false).await?;
        let started = Instant::now();
        let outcome = sqlx::query_as::<_, (Option<String>, Option<String>, bool)>(&format!(
            "SELECT a.usename::text, a.application_name, {}($1) \
             FROM pg_stat_activity a \
             WHERE a.pid = $1 AND a.datname = current_database() AND a.pid <> pg_backend_pid()",
            signal.function()
        ))
        .bind(pid)
        .fetch_optional(&mut conn)
        .await;
        close(conn).await;
        let elapsed = started.elapsed().as_millis() as u64;

        let (target, result) = match outcome {
            Ok(None) => (None, Err(AppError::NotFound("session"))),
            Ok(Some((user, app, true))) => (Some((user, app)), Ok(())),
            Ok(Some((user, app, false))) => (
                Some((user, app)),
                Err(AppError::Conflict(
                    "PostgreSQL could not signal the session; it may have just ended".into(),
                )),
            ),
            Err(e) => (None, Err(signal_error(e, signal))),
        };

        let who = match &target {
            Some((user, app)) => format!(
                " ({}, {})",
                user.as_deref().unwrap_or("unknown role"),
                app.as_deref()
                    .filter(|a| !a.is_empty())
                    .unwrap_or("no application name")
            ),
            None => String::new(),
        };
        let script = format!(
            "-- {} session {pid}{who}\nSELECT {}({pid});",
            signal.verb(),
            signal.function()
        );
        let error = result.as_ref().err().map(ToString::to_string);
        tracing::info!(%user_id, %connection_id, pid, signal = signal.function(), ok = error.is_none(), "backend signal");
        LibraryService::new(self.state)
            .record_script(
                user_id,
                connection_id,
                HistoryKind::Admin,
                &script,
                elapsed,
                Some(i64::from(error.is_none())),
                error.as_deref(),
            )
            .await;

        result.map(|()| SignalResponse {
            pid,
            signal: signal.function(),
            signalled: true,
        })
    }
}

fn signal_error(e: sqlx::Error, signal: Signal) -> AppError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("42501") {
            let message = db
                .try_downcast_ref::<PgDatabaseError>()
                .map(|p| p.message().to_string())
                .unwrap_or_default();
            return AppError::Forbidden(format!(
                "PostgreSQL refused to {} this session: {message}. The connected role may only signal \
                 sessions of roles it belongs to, unless it is a member of pg_signal_backend; \
                 superuser sessions need a superuser.",
                signal.verb().to_lowercase()
            ));
        }
    }
    target_db::query_error(e)
}

async fn privileges(conn: &mut PgConnection, record: &ConnectionRecord) -> AppResult<Privileges> {
    let mut p = sqlx::query_as::<_, Privileges>(PRIVILEGES_SQL)
        .fetch_one(&mut *conn)
        .await
        .map_err(target_db::query_error)?;
    p.read_only_connection = record.read_only;
    Ok(p)
}

async fn sessions(conn: &mut PgConnection) -> AppResult<Vec<Session>> {
    sqlx::query_as::<_, Session>(ACTIVITY_SQL)
        .bind(MAX_SESSIONS)
        .fetch_all(conn)
        .await
        .map_err(target_db::query_error)
}

async fn load_sizes(conn: &mut PgConnection, schema: Option<&str>) -> AppResult<SizesResponse> {
    let (database, database_bytes): (String, i64) =
        sqlx::query_as("SELECT current_database()::text, pg_database_size(current_database())")
            .fetch_one(&mut *conn)
            .await
            .map_err(target_db::query_error)?;

    let schemas = sqlx::query_as::<_, SchemaSize>(&format!(
        "SELECT n.nspname::text AS schema, \
                COALESCE(sum(pg_total_relation_size(c.oid)), 0)::bigint AS total_bytes, \
                count(c.oid)::bigint AS tables \
         FROM pg_namespace n \
         LEFT JOIN pg_class c ON c.relnamespace = n.oid AND c.relkind IN ('r', 'm') \
         WHERE {USER_SCHEMAS} \
         GROUP BY n.nspname ORDER BY 2 DESC, 1"
    ))
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    // Leaf tables and materialized views (partitioned parents hold no data;
    // their partitions are listed).
    let tables = sqlx::query_as::<_, TableSize>(&format!(
        "SELECT n.nspname::text AS schema, c.relname::text AS name, \
                CASE c.relkind WHEN 'm' THEN 'materialized_view' ELSE 'table' END AS kind, \
                pg_total_relation_size(c.oid) AS total_bytes, \
                pg_table_size(c.oid) AS table_bytes, pg_indexes_size(c.oid) AS index_bytes, \
                CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END AS estimated_rows, \
                s.n_live_tup AS live_rows, s.n_dead_tup AS dead_rows, \
                GREATEST(s.last_vacuum, s.last_autovacuum) AS last_vacuum, \
                GREATEST(s.last_analyze, s.last_autoanalyze) AS last_analyze \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         LEFT JOIN pg_stat_all_tables s ON s.relid = c.oid \
         WHERE c.relkind IN ('r', 'm') AND {USER_SCHEMAS} AND ($1::text IS NULL OR n.nspname = $1) \
         ORDER BY 4 DESC, 1, 2 LIMIT $2"
    ))
    .bind(schema)
    .bind(MAX_SIZE_ROWS)
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    let indexes = sqlx::query_as::<_, IndexSize>(&format!(
        "SELECT n.nspname::text AS schema, t.relname::text AS \"table\", i.relname::text AS name, \
                pg_relation_size(i.oid) AS bytes, s.idx_scan AS scans, \
                x.indisunique AS \"unique\", x.indisprimary AS \"primary\" \
         FROM pg_index x \
         JOIN pg_class i ON i.oid = x.indexrelid \
         JOIN pg_class t ON t.oid = x.indrelid \
         JOIN pg_namespace n ON n.oid = t.relnamespace \
         LEFT JOIN pg_stat_all_indexes s ON s.indexrelid = x.indexrelid \
         WHERE t.relkind IN ('r', 'm') AND {USER_SCHEMAS} AND ($1::text IS NULL OR n.nspname = $1) \
         ORDER BY 4 DESC, 1, 2, 3 LIMIT $2"
    ))
    .bind(schema)
    .bind(MAX_INDEX_ROWS)
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    Ok(SizesResponse {
        database,
        database_bytes,
        schemas,
        tables,
        indexes,
    })
}

const NOT_INSTALLED: &str = "The pg_stat_statements extension is not installed in this database. \
     A superuser can add `pg_stat_statements` to shared_preload_libraries, restart the server and \
     run CREATE EXTENSION pg_stat_statements.";

/// Statement statistics, or why they are unavailable.
async fn load_top_queries(
    conn: &mut PgConnection,
    sort: TopQueriesSort,
) -> AppResult<Result<Vec<TopQuery>, String>> {
    let schema: Option<String> = sqlx::query_scalar(
        "SELECT n.nspname::text FROM pg_extension e JOIN pg_namespace n ON n.oid = e.extnamespace \
         WHERE e.extname = 'pg_stat_statements'",
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(target_db::query_error)?;
    let Some(schema) = schema else {
        return Ok(Err(NOT_INSTALLED.into()));
    };
    let view = format!("{}.pg_stat_statements", quote_ident(&schema));

    // Column names changed in PostgreSQL 13 (total_time -> total_exec_time).
    let modern: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_attribute \
                        WHERE attrelid = $1::regclass AND attname = 'total_exec_time')",
    )
    .bind(&view)
    .fetch_one(&mut *conn)
    .await
    .map_err(target_db::query_error)?;
    let (total, mean, max) = if modern {
        ("total_exec_time", "mean_exec_time", "max_exec_time")
    } else {
        ("total_time", "mean_time", "max_time")
    };
    let order = match sort {
        TopQueriesSort::Total => "total_ms",
        TopQueriesSort::Mean => "mean_ms",
        TopQueriesSort::Calls => "calls",
    };
    let rows = sqlx::query_as::<_, TopQuery>(&format!(
        "SELECT s.queryid::text AS query_id, \
                CASE WHEN s.query = '<insufficient privilege>' THEN NULL ELSE left(s.query, 2000) END AS query, \
                s.calls::bigint AS calls, s.{total}::float8 AS total_ms, s.{mean}::float8 AS mean_ms, \
                s.{max}::float8 AS max_ms, s.rows::bigint AS rows, \
                s.shared_blks_hit::bigint AS shared_hit, s.shared_blks_read::bigint AS shared_read \
         FROM {view} s \
         WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
         ORDER BY {order} DESC NULLS LAST LIMIT {MAX_TOP_QUERIES}"
    ))
    .fetch_all(&mut *conn)
    .await;
    match rows {
        Ok(rows) => Ok(Ok(rows)),
        // Not preloaded (55000) or no permission (42501): explain, don't fail.
        Err(sqlx::Error::Database(db))
            if matches!(db.code().as_deref(), Some("55000" | "42501")) =>
        {
            Ok(Err(format!(
                "pg_stat_statements is installed but cannot be read: {}",
                db.message()
            )))
        }
        Err(e) => Err(target_db::query_error(e)),
    }
}

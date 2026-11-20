//! Data Studio: browse, preview, apply and export the rows of a table.
//!
//! * Browsing, previews and exports run on a read-only connection inside
//!   `BEGIN READ ONLY`.
//! * Applying a change set is a privileged workflow: it requires a
//!   read-write connection, typed confirmation on production connections and
//!   runs every statement in one transaction. Updates and deletes must affect
//!   exactly one row, otherwise everything is rolled back.
//! * All SQL comes from [`sql_builder`], which checks identifiers against the
//!   catalog loaded here and binds every value.

use std::time::{Duration, Instant};

use axum::body::Bytes;
use futures_util::TryStreamExt;
use sqlx::{postgres::PgRow, Connection, Executor, PgConnection, Row};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::{
        connection::{ConnectionRecord, Environment},
        data::{
            AppliedChange, ApplyFailure, ApplyRequest, ApplyResponse, BrowseRequest,
            BrowseResponse, ChangeKind, DataColumn, ExportFormat, ExportRequest, Pagination,
            PreviewRequest, PreviewResponse, PreviewStatement, DEFAULT_PAGE_SIZE,
        },
        library::HistoryKind,
    },
    services::{
        auth::AuthUser,
        connections::ConnectionService,
        library::LibraryService,
        schema::RELKIND_LABEL,
        sql_builder::{
            self, group_is_empty, Column, Cursor, KeyKind, RowQuery, Sql, Table, COUNT_CAP,
        },
        target_db::{self, TargetSpec},
    },
    state::AppState,
};

/// Exports stop after this many rows.
pub const MAX_EXPORT_ROWS: u64 = 100_000;
/// A browse page stops early once its values exceed this many bytes.
const MAX_PAGE_BYTES: usize = 16 * 1024 * 1024;
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
const EXPORT_CHUNK_BYTES: usize = 64 * 1024;
const SQLSTATE_QUERY_CANCELED: &str = "57014";

/// A streamed export: the handler turns `body` into the response body.
pub struct ExportStream {
    pub filename: String,
    pub content_type: &'static str,
    pub body: mpsc::Receiver<Result<Bytes, std::io::Error>>,
}

#[derive(sqlx::FromRow)]
struct ColumnRow {
    name: String,
    data_type: String,
    cast_type: String,
    base_type: String,
    category: String,
    nullable: bool,
    has_default: bool,
    identity: String,
    generated: bool,
    enum_values: Vec<String>,
}

/// Loads the catalog model of a table, view or foreign table.
async fn load_table(conn: &mut PgConnection, schema: &str, name: &str) -> AppResult<(Table, i64)> {
    let relation = sqlx::query_as::<_, (i64, String, bool, bool, bool)>(&format!(
        "SELECT c.oid::bigint, {RELKIND_LABEL}, \
                has_table_privilege(c.oid, 'INSERT'), has_table_privilege(c.oid, 'UPDATE'), \
                has_table_privilege(c.oid, 'DELETE') \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r','p','v','m','f')"
    ))
    .bind(schema)
    .bind(name)
    .fetch_optional(&mut *conn)
    .await
    .map_err(target_db::query_error)?
    .ok_or(AppError::NotFound("table"))?;
    let (oid, kind, can_insert, can_update, can_delete) = relation;

    // 1042/1014 are bpchar and bpchar[], 1560/1561 bit and bit[]: without a
    // length their bare names mean length 1, so casts use the unbounded
    // spelling and assignment still enforces the declared length.
    let columns = sqlx::query_as::<_, ColumnRow>(
        "SELECT a.attname::text AS name, format_type(a.atttypid, a.atttypmod) AS data_type, \
                CASE a.atttypid::int WHEN 1042 THEN 'bpchar' WHEN 1014 THEN 'bpchar[]' \
                     WHEN 1560 THEN 'varbit' WHEN 1561 THEN 'varbit[]' \
                     ELSE format_type(a.atttypid, NULL) END AS cast_type, \
                bt.typname::text AS base_type, t.typcategory::text AS category, \
                NOT a.attnotnull AS nullable, a.atthasdef AS has_default, \
                a.attidentity::text AS identity, a.attgenerated <> '' AS generated, \
                ARRAY(SELECT e.enumlabel::text FROM pg_enum e WHERE e.enumtypid = bt.oid \
                      ORDER BY e.enumsortorder) AS enum_values \
         FROM pg_attribute a \
         JOIN pg_type t ON t.oid = a.atttypid \
         JOIN pg_type bt ON bt.oid = CASE WHEN t.typtype = 'd' THEN t.typbasetype ELSE t.oid END \
         WHERE a.attrelid = $1::bigint::oid AND a.attnum > 0 AND NOT a.attisdropped \
         ORDER BY a.attnum",
    )
    .bind(oid)
    .fetch_all(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    // The primary key, or else the narrowest valid unique index whose key
    // columns are all NOT NULL (no expressions, no predicate).
    let key = sqlx::query_as::<_, (Vec<String>, bool)>(
        "SELECT ARRAY(SELECT a.attname::text \
                      FROM unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum \
                      WHERE k.ord <= i.indnkeyatts ORDER BY k.ord), \
                i.indisprimary \
         FROM pg_index i \
         WHERE i.indrelid = $1::bigint::oid AND i.indisunique AND i.indisvalid \
           AND i.indimmediate AND i.indpred IS NULL AND i.indexprs IS NULL \
           AND NOT EXISTS (SELECT 1 FROM unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, ord) \
                           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum \
                           WHERE k.ord <= i.indnkeyatts AND NOT a.attnotnull) \
         ORDER BY i.indisprimary DESC, i.indnkeyatts, i.indexrelid \
         LIMIT 1",
    )
    .bind(oid)
    .fetch_optional(&mut *conn)
    .await
    .map_err(target_db::query_error)?;

    let columns: Vec<Column> = columns
        .into_iter()
        .map(|c| Column {
            name: c.name,
            data_type: c.data_type,
            cast_type: c.cast_type,
            base_type: c.base_type,
            category: c.category.chars().next().unwrap_or('X'),
            nullable: c.nullable,
            has_default: c.has_default,
            identity: c.identity.chars().next(),
            generated: c.generated,
            enum_values: c.enum_values,
        })
        .collect();
    let (key, key_kind) = match key {
        Some((names, primary)) => {
            let indexes: Option<Vec<usize>> = names
                .iter()
                .map(|n| columns.iter().position(|c| &c.name == n))
                .collect();
            match indexes {
                Some(ix) if !ix.is_empty() => (
                    ix,
                    Some(if primary {
                        KeyKind::PrimaryKey
                    } else {
                        KeyKind::Unique
                    }),
                ),
                _ => (Vec::new(), None),
            }
        }
        None => (Vec::new(), None),
    };
    Ok((
        Table {
            schema: schema.to_string(),
            name: name.to_string(),
            kind,
            columns,
            key,
            key_kind,
            can_insert,
            can_update,
            can_delete,
        },
        oid,
    ))
}

/// Maps a target error. The executed text is never shown, so positions
/// into it are dropped.
fn target_error(err: sqlx::Error, timeout: Duration) -> AppError {
    if let sqlx::Error::Database(db) = &err {
        if db.code().as_deref() == Some(SQLSTATE_QUERY_CANCELED) {
            return AppError::QueryTimeout {
                timeout_secs: timeout.as_secs(),
                execution_time_ms: timeout.as_millis() as u64,
            };
        }
    }
    match target_db::query_error(err) {
        AppError::Sql(mut info) => {
            info.position = None;
            AppError::Sql(info)
        }
        other => other,
    }
}

fn query(sql: &Sql) -> sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments> {
    let mut q = sqlx::query(&sql.text);
    for p in &sql.params {
        q = q.bind(p.as_deref());
    }
    q
}

fn text_row(row: &PgRow) -> Result<Vec<Option<String>>, sqlx::Error> {
    (0..row.len())
        .map(|i| row.try_get::<Option<String>, _>(i))
        .collect()
}

async fn close(conn: PgConnection) {
    let _ = tokio::time::timeout(Duration::from_secs(2), conn.close()).await;
}

fn data_column(table: &Table, index: usize) -> DataColumn {
    let c = &table.columns[index];
    DataColumn {
        name: c.name.clone(),
        data_type: c.data_type.clone(),
        type_name: c.base_type.clone(),
        kind: c.kind(),
        nullable: c.nullable,
        has_default: c.has_default,
        is_key: table.key.contains(&index),
        identity: c.identity_label(),
        generated: c.generated,
        editable: c.editable(),
        enum_values: c.enum_values.clone(),
    }
}

/// Typed confirmation for privileged operations (architecture principle 4).
pub fn check_confirmation(record: &ConnectionRecord, confirm: Option<&str>) -> AppResult<()> {
    let confirm = confirm.map(str::trim).unwrap_or_default();
    match record.environment() {
        Environment::Production if confirm != record.name.trim() => Err(AppError::validation(
            "confirm",
            "type the connection name to confirm changes on a production connection",
        )),
        Environment::Staging if confirm.is_empty() => Err(AppError::validation(
            "confirm",
            "confirm changes on a staging connection",
        )),
        _ => Ok(()),
    }
}

const READ_ONLY_CONNECTION: &str =
    "This connection is read-only. Enable read-write mode in the connection settings to edit data.";

pub struct DataStudioService<'a> {
    state: &'a AppState,
}

impl<'a> DataStudioService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    async fn connect(&self, record: &ConnectionRecord, read_only: bool) -> AppResult<PgConnection> {
        let service = ConnectionService::new(self.state);
        let secret = service.decrypt_secret(record)?;
        let mut spec = TargetSpec::from_record(record, &secret);
        spec.read_only = read_only || record.read_only;
        let mut conn = target_db::connect(&self.state.config, &spec).await?;
        target_db::set_statement_timeout(&mut conn, CATALOG_TIMEOUT).await?;
        Ok(conn)
    }

    async fn open_read_only(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
    ) -> AppResult<(PgConnection, ConnectionRecord)> {
        let record = ConnectionService::new(self.state)
            .get_record(user_id, connection_id)
            .await?;
        let conn = self.connect(&record, true).await?;
        Ok((conn, record))
    }

    pub async fn browse(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        req: &BrowseRequest,
    ) -> AppResult<BrowseResponse> {
        self.state.query_limiter.check(&user.user_id.to_string())?;
        let cursor = req.cursor.as_deref().map(Cursor::decode).transpose()?;
        let (mut conn, record) = self.open_read_only(user.user_id, connection_id).await?;
        let (table, oid) = load_table(&mut conn, &req.schema, &req.table).await?;
        let q = RowQuery {
            filter: req.filter.as_ref(),
            sort: &req.sort,
            search: req.search.as_deref(),
        };
        let page_size = req.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
        let (sql, pagination) = sql_builder::build_browse(&table, &q, cursor.as_ref(), page_size)?;
        let count_sql = sql_builder::build_count(&table, &q)?;
        let timeout = self.state.config.query_default_timeout;

        conn.execute("BEGIN READ ONLY")
            .await
            .map_err(target_db::query_error)?;
        target_db::set_local_timeouts(&mut conn, timeout).await?;
        let started = Instant::now();

        // The count runs on the first page only.
        let (total_rows, total_exact) = if cursor.is_none() {
            let counted: i64 = match query(&count_sql).fetch_one(&mut conn).await {
                Ok(row) => row.try_get(0).map_err(target_db::query_error)?,
                Err(err) => return Err(target_error(err, timeout)),
            };
            if counted <= COUNT_CAP {
                (Some(counted), true)
            } else {
                let unfiltered = q.filter.map_or(true, group_is_empty)
                    && q.search.map_or(true, |s| s.trim().is_empty());
                let estimate = if unfiltered {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT GREATEST(reltuples, 0)::bigint FROM pg_class WHERE oid = $1::bigint::oid",
                    )
                    .bind(oid)
                    .fetch_one(&mut conn)
                    .await
                    .unwrap_or(0)
                } else {
                    0
                };
                (Some(estimate.max(counted)), false)
            }
        } else {
            (None, true)
        };

        let mut rows: Vec<Vec<Option<String>>> = Vec::new();
        let mut bytes = 0usize;
        let mut has_more = false;
        let mut truncated = false;
        let fetched: Result<(), sqlx::Error> = async {
            let mut stream = query(&sql).fetch(&mut conn);
            while let Some(row) = stream.try_next().await? {
                if rows.len() as u32 >= page_size {
                    has_more = true;
                    break;
                }
                if bytes >= MAX_PAGE_BYTES {
                    has_more = true;
                    truncated = true;
                    break;
                }
                let values = text_row(&row)?;
                bytes += values
                    .iter()
                    .map(|v| v.as_ref().map_or(4, String::len))
                    .sum::<usize>();
                rows.push(values);
            }
            Ok(())
        }
        .await;
        let elapsed = started.elapsed().as_millis() as u64;
        match fetched {
            // Unread rows may still be in flight: closing the socket ends the
            // read-only transaction without draining them.
            Ok(()) if has_more => drop(conn),
            Ok(()) => {
                let _ = conn.execute("ROLLBACK").await;
                close(conn).await;
            }
            Err(err) => return Err(target_error(err, timeout)),
        }

        let next_cursor = match (has_more, rows.last()) {
            (true, Some(last)) => Some(match pagination {
                Pagination::Keyset => Cursor::After(
                    table
                        .key
                        .iter()
                        .map(|&i| last[i].clone().unwrap_or_default())
                        .collect(),
                ),
                Pagination::Offset => {
                    let offset = match cursor {
                        Some(Cursor::Offset(o)) => o,
                        _ => 0,
                    };
                    Cursor::Offset(offset + rows.len() as u64)
                }
            }),
            _ => None,
        }
        .map(|c| c.encode());

        let read_only_reason = if record.read_only {
            Some(READ_ONLY_CONNECTION.to_string())
        } else {
            table.read_only_reason().map(str::to_string)
        };
        Ok(BrowseResponse {
            schema: table.schema.clone(),
            table: table.name.clone(),
            kind: table.kind.clone(),
            columns: (0..table.columns.len())
                .map(|i| data_column(&table, i))
                .collect(),
            key_columns: table.key_columns().map(|c| c.name.clone()).collect(),
            key_kind: table.key_kind.map(KeyKind::as_str),
            rows,
            pagination,
            next_cursor,
            total_rows,
            total_exact,
            editable: read_only_reason.is_none(),
            read_only_reason,
            can_insert: table.can_insert,
            can_update: table.can_update,
            can_delete: table.can_delete,
            sql: sql.display,
            truncated,
            execution_time_ms: elapsed,
        })
    }

    /// Renders the SELECT for a filter and the statements for a change set
    /// without running them.
    pub async fn preview(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        req: &PreviewRequest,
    ) -> AppResult<PreviewResponse> {
        self.state
            .connect_limiter
            .check(&user.user_id.to_string())?;
        let (mut conn, _) = self.open_read_only(user.user_id, connection_id).await?;
        let loaded = load_table(&mut conn, &req.schema, &req.table).await;
        close(conn).await;
        let (table, _) = loaded?;
        let q = RowQuery {
            filter: req.filter.as_ref(),
            sort: &req.sort,
            search: req.search.as_deref(),
        };
        let (select, _) = sql_builder::build_browse(
            &table,
            &q,
            None,
            req.page_size.unwrap_or(DEFAULT_PAGE_SIZE),
        )?;
        let statements = match &req.changes {
            Some(changes) if !changes.is_empty() => sql_builder::build_changes(&table, changes)?
                .into_iter()
                .map(|s| PreviewStatement {
                    kind: s.kind,
                    index: s.index,
                    sql: s.sql.display,
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(PreviewResponse {
            select_sql: select.display,
            statements,
        })
    }

    /// Applies a change set in one transaction and records it in history.
    pub async fn apply(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        req: &ApplyRequest,
    ) -> AppResult<ApplyResponse> {
        self.state.query_limiter.check(&user.user_id.to_string())?;
        let record = ConnectionService::new(self.state)
            .get_record(user.user_id, connection_id)
            .await?;
        if record.read_only {
            return Err(AppError::Forbidden(READ_ONLY_CONNECTION.into()));
        }
        check_confirmation(&record, req.confirm.as_deref())?;

        let mut conn = self.connect(&record, false).await?;
        let (table, _) = load_table(&mut conn, &req.schema, &req.table).await?;
        let statements = sql_builder::build_changes(&table, &req.changes)?;
        let qualified = table.qualified();
        for (needed, allowed, privilege) in [
            (!req.changes.inserts.is_empty(), table.can_insert, "INSERT"),
            (!req.changes.updates.is_empty(), table.can_update, "UPDATE"),
            (!req.changes.deletes.is_empty(), table.can_delete, "DELETE"),
        ] {
            if needed && !allowed {
                close(conn).await;
                return Err(AppError::Forbidden(format!(
                    "The connection's role does not have {privilege} privilege on {qualified}."
                )));
            }
        }

        let timeout = self.state.config.query_default_timeout;
        conn.execute("BEGIN")
            .await
            .map_err(target_db::query_error)?;
        target_db::set_local_timeouts(&mut conn, timeout).await?;
        let started = Instant::now();

        let mut applied = Vec::with_capacity(statements.len());
        let mut failure: Option<ApplyFailure> = None;
        for st in &statements {
            match query(&st.sql).fetch_all(&mut conn).await {
                Ok(rows) if rows.len() == 1 => {
                    let row = if st.kind == ChangeKind::Delete {
                        None
                    } else {
                        Some(text_row(&rows[0]).map_err(target_db::query_error)?)
                    };
                    applied.push(AppliedChange {
                        kind: st.kind,
                        index: st.index,
                        row,
                    });
                }
                Ok(rows) => {
                    let message = match (rows.len(), st.guarded) {
                        (0, true) => "The row was changed or deleted after it was loaded. Reload the data and try again.".to_string(),
                        (0, false) => "The row no longer exists. Reload the data and try again.".to_string(),
                        (n, _) => format!("The change matched {n} rows instead of one."),
                    };
                    failure = Some(ApplyFailure {
                        kind: Some(st.kind),
                        index: Some(st.index),
                        conflict: true,
                        message,
                        sql_error: None,
                        statement: st.sql.display.clone(),
                    });
                    break;
                }
                Err(err) => {
                    let error = target_error(err, timeout);
                    failure = Some(ApplyFailure {
                        kind: Some(st.kind),
                        index: Some(st.index),
                        conflict: false,
                        message: error.to_string(),
                        sql_error: match error {
                            AppError::Sql(info) => Some(info),
                            _ => None,
                        },
                        statement: st.sql.display.clone(),
                    });
                    break;
                }
            }
        }

        if failure.is_some() {
            // Dropping the connection aborts the transaction server-side.
            drop(conn);
        } else if let Err(err) = conn.execute("COMMIT").await {
            let error = target_error(err, timeout);
            failure = Some(ApplyFailure {
                kind: None,
                index: None,
                conflict: false,
                message: error.to_string(),
                sql_error: match error {
                    AppError::Sql(info) => Some(info),
                    _ => None,
                },
                statement: "COMMIT;".into(),
            });
            drop(conn);
        } else {
            close(conn).await;
        }
        let elapsed = started.elapsed().as_millis() as u64;
        let committed = failure.is_none();
        let rows_affected = if committed { applied.len() as u64 } else { 0 };
        if !committed {
            applied.clear();
        }

        let script = format!(
            "-- Data Studio: {} change(s) to {qualified}\n{}",
            statements.len(),
            statements
                .iter()
                .map(|s| s.sql.display.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let history_id = LibraryService::new(self.state)
            .record_script(
                user.user_id,
                connection_id,
                HistoryKind::DataApply,
                &script,
                elapsed,
                committed.then_some(rows_affected as i64),
                failure.as_ref().map(|f| f.message.as_str()),
            )
            .await;
        tracing::info!(
            user_id = %user.user_id, %connection_id, changes = statements.len(), committed,
            "data studio change set applied"
        );

        Ok(ApplyResponse {
            committed,
            applied,
            failure,
            rows_affected,
            execution_time_ms: elapsed,
            history_id,
        })
    }

    /// Starts a streamed export. Validation and connection errors are
    /// returned before any bytes are sent; a failure mid-stream aborts the
    /// response so a partial file is never mistaken for a complete one.
    pub async fn export(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        req: &ExportRequest,
    ) -> AppResult<ExportStream> {
        self.state.query_limiter.check(&user.user_id.to_string())?;
        let (mut conn, _) = self.open_read_only(user.user_id, connection_id).await?;
        let (table, _) = load_table(&mut conn, &req.schema, &req.table).await?;
        let mut columns: Vec<usize> = match &req.columns {
            Some(names) => names
                .iter()
                .map(|n| table.column(n).map(|(i, _)| i))
                .collect::<AppResult<_>>()?,
            None => (0..table.columns.len()).collect(),
        };
        if req.format == ExportFormat::Sql {
            // Generated columns cannot be inserted.
            columns.retain(|&i| !table.columns[i].generated);
        }
        if columns.is_empty() {
            return Err(AppError::validation(
                "columns",
                "no exportable columns selected",
            ));
        }
        let q = RowQuery {
            filter: req.filter.as_ref(),
            sort: &req.sort,
            search: req.search.as_deref(),
        };
        let sql =
            sql_builder::build_export(&table, &q, req.keys.as_deref(), &columns, MAX_EXPORT_ROWS)?;

        let timeout = self.state.config.query_max_timeout;
        conn.execute("BEGIN READ ONLY")
            .await
            .map_err(target_db::query_error)?;
        target_db::set_local_timeouts(&mut conn, timeout).await?;

        let (tx, rx) = mpsc::channel(8);
        let format = req.format;
        let user_id = user.user_id;
        tokio::spawn(async move {
            match write_export(&mut conn, &sql, &table, &columns, format, &tx).await {
                Ok(rows) => {
                    let _ = conn.execute("ROLLBACK").await;
                    close(conn).await;
                    tracing::info!(%user_id, %connection_id, rows, "data studio export finished");
                }
                Err(err) => {
                    drop(conn);
                    tracing::warn!(%user_id, %connection_id, error = %err, "data studio export failed");
                    let _ = tx.send(Err(std::io::Error::other(err))).await;
                }
            }
        });

        let safe_name: String = req
            .table
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Ok(ExportStream {
            filename: format!("{safe_name}.{}", req.format.extension()),
            content_type: req.format.content_type(),
            body: rx,
        })
    }
}

#[derive(Debug, thiserror::Error)]
enum ExportError {
    #[error("{0}")]
    Query(#[from] sqlx::Error),
    #[error("the client disconnected")]
    Disconnected,
}

async fn send_chunk(
    tx: &mpsc::Sender<Result<Bytes, std::io::Error>>,
    buf: &mut String,
) -> Result<(), ExportError> {
    let chunk = Bytes::from(std::mem::take(buf));
    tx.send(Ok(chunk))
        .await
        .map_err(|_| ExportError::Disconnected)
}

async fn write_export(
    conn: &mut PgConnection,
    sql: &Sql,
    table: &Table,
    columns: &[usize],
    format: ExportFormat,
    tx: &mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Result<u64, ExportError> {
    let cols: Vec<&Column> = columns.iter().map(|&i| &table.columns[i]).collect();
    let mut buf = String::new();
    match format {
        ExportFormat::Csv => {
            buf.push_str(&sql_builder::csv_line(
                cols.iter().map(|c| Some(c.name.as_str())),
            ));
        }
        ExportFormat::Json => buf.push('['),
        ExportFormat::Sql => {
            buf.push_str(&format!(
                "-- Exported from {} by QueryLab\n",
                table.qualified()
            ));
        }
    }
    let mut count = 0u64;
    let mut stream = query(sql).fetch(conn);
    while let Some(row) = stream.try_next().await? {
        let values = text_row(&row)?;
        match format {
            ExportFormat::Csv => {
                buf.push_str(&sql_builder::csv_line(values.iter().map(|v| v.as_deref())));
            }
            ExportFormat::Json => {
                if count > 0 {
                    buf.push(',');
                }
                buf.push_str("\n  {");
                for (i, (col, value)) in cols.iter().zip(&values).enumerate() {
                    if i > 0 {
                        buf.push_str(", ");
                    }
                    buf.push_str(&serde_json::Value::String(col.name.clone()).to_string());
                    buf.push_str(": ");
                    buf.push_str(&sql_builder::json_cell(value.as_deref(), col).to_string());
                }
                buf.push('}');
            }
            ExportFormat::Sql => {
                buf.push_str(&sql_builder::sql_insert_line(table, &cols, &values));
            }
        }
        count += 1;
        if buf.len() >= EXPORT_CHUNK_BYTES {
            send_chunk(tx, &mut buf).await?;
        }
    }
    if format == ExportFormat::Json {
        buf.push_str(if count > 0 { "\n]\n" } else { "]\n" });
    }
    send_chunk(tx, &mut buf).await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn record(environment: &str, name: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: name.into(),
            database_type: "postgresql".into(),
            host: "db".into(),
            port: 5432,
            database_name: "app".into(),
            username: "app".into(),
            ssl_mode: "require".into(),
            read_only: false,
            encrypted_credentials: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_connected_at: None,
            environment: environment.into(),
            group_id: None,
            color: None,
            ssl_root_cert: None,
            last_status: None,
            last_status_at: None,
            last_latency_ms: None,
            last_error: None,
        }
    }

    #[test]
    fn production_requires_the_connection_name() {
        let prod = record("production", "Billing DB");
        assert!(check_confirmation(&prod, None).is_err());
        assert!(check_confirmation(&prod, Some("billing db")).is_err());
        assert!(check_confirmation(&prod, Some(" Billing DB ")).is_ok());

        let staging = record("staging", "Stage");
        assert!(check_confirmation(&staging, Some("  ")).is_err());
        assert!(check_confirmation(&staging, Some("yes")).is_ok());

        assert!(check_confirmation(&record("development", "Dev"), None).is_ok());
    }
}

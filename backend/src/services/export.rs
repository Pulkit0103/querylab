//! Streamed exports of a table (all, filtered or selected rows) or of the
//! full result of one read-only statement, as CSV, JSON, SQL `INSERT`
//! statements or Excel.
//!
//! Rows are read from a cursor inside `BEGIN READ ONLY` and encoded on the
//! fly, so memory stays flat whatever the size of the export. Excel files
//! are built with a constant-memory worksheet (rows go to a temporary file)
//! and capped at [`MAX_XLSX_ROWS`].
//!
//! Validation and connection errors are returned before any bytes are sent;
//! a failure mid-stream aborts the response so a partial file is never
//! mistaken for a complete one.

use std::time::Instant;

use axum::body::Bytes;
use futures_util::TryStreamExt;
use rust_xlsxwriter::{Format, Workbook};
use serde_json::Value;
use sqlx::{
    postgres::{PgRow, PgTypeInfo},
    Column as _, Executor, PgConnection, Row, Statement, TypeInfo,
};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    database::data_jobs as jobs_repo,
    errors::{AppError, AppResult},
    models::{
        data::{ExportFormat, TableSource},
        data_jobs::{JobKind, JobStatus, JobView},
    },
    services::{
        auth::AuthUser,
        connections::ConnectionService,
        data_studio::{close, load_table, DataStudioService},
        sql_builder::{self, Column, RowQuery, Sql},
        target_db,
        value_decode::decode_cell,
    },
    state::AppState,
};

/// Streamed formats stop after this many rows.
pub const MAX_EXPORT_ROWS: u64 = 1_000_000;
/// Excel's practical limit for one sheet.
pub const MAX_XLSX_ROWS: u64 = 200_000;
const EXPORT_CHUNK_BYTES: usize = 64 * 1024;
/// Excel's limit for the text of one cell.
const XLSX_MAX_STRING_CHARS: usize = 32_767;

pub fn row_limit(format: ExportFormat) -> u64 {
    match format {
        ExportFormat::Xlsx => MAX_XLSX_ROWS,
        _ => MAX_EXPORT_ROWS,
    }
}

/// A streamed export: the handler turns `body` into the response body.
pub struct ExportStream {
    pub filename: String,
    pub content_type: &'static str,
    pub body: mpsc::Receiver<Result<Bytes, std::io::Error>>,
}

/// Every column of a row as PostgreSQL's text representation.
pub(crate) fn text_row(row: &PgRow) -> Result<Vec<Option<String>>, sqlx::Error> {
    (0..row.len())
        .map(|i| row.try_get::<Option<String>, _>(i))
        .collect()
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim_matches(['_', '.']).is_empty() {
        "export".into()
    } else {
        cleaned
    }
}

/// Column metadata for a query result, enough to type JSON, SQL and Excel
/// values.
fn query_column(name: &str, info: &PgTypeInfo) -> Column {
    let type_name = info.name().to_ascii_lowercase();
    let (base, category) = match type_name.as_str() {
        "int2" | "int4" | "int8" | "float4" | "float8" | "numeric" | "oid" => {
            (type_name.clone(), 'N')
        }
        "bool" => (type_name.clone(), 'B'),
        "json" | "jsonb" | "uuid" => (type_name.clone(), 'U'),
        t if t.ends_with("[]") => (type_name.clone(), 'A'),
        _ => (type_name.clone(), 'S'),
    };
    Column {
        name: name.to_string(),
        data_type: type_name.clone(),
        cast_type: type_name,
        base_type: base,
        category,
        nullable: true,
        has_default: false,
        identity: None,
        generated: false,
        enum_values: Vec::new(),
    }
}

#[derive(Debug, thiserror::Error)]
enum ExportError {
    #[error("{0}")]
    Query(#[from] sqlx::Error),
    #[error("could not build the Excel file: {0}")]
    Xlsx(String),
    #[error("the client disconnected")]
    Disconnected,
}

impl From<rust_xlsxwriter::XlsxError> for ExportError {
    fn from(e: rust_xlsxwriter::XlsxError) -> Self {
        ExportError::Xlsx(e.to_string())
    }
}

/// Encodes rows of text values into the chosen format.
struct Encoder {
    format: ExportFormat,
    columns: Vec<Column>,
    insert_target: String,
    buf: String,
    count: u64,
    workbook: Option<Workbook>,
}

impl Encoder {
    fn new(
        format: ExportFormat,
        columns: Vec<Column>,
        insert_target: String,
        source: &str,
    ) -> Result<Self, ExportError> {
        let mut enc = Self {
            format,
            columns,
            insert_target,
            buf: String::new(),
            count: 0,
            workbook: None,
        };
        match format {
            ExportFormat::Csv => {
                enc.buf.push_str(&sql_builder::csv_line(
                    enc.columns.iter().map(|c| Some(c.name.as_str())),
                ));
            }
            ExportFormat::Json => enc.buf.push('['),
            ExportFormat::Sql => {
                enc.buf
                    .push_str(&format!("-- Exported from {source} by QueryLab\n"));
            }
            ExportFormat::Xlsx => {
                let mut workbook = Workbook::new();
                let bold = Format::new().set_bold();
                let sheet = workbook.add_worksheet_with_constant_memory();
                for (i, c) in enc.columns.iter().enumerate() {
                    sheet.write_string_with_format(0, xlsx_col(i)?, truncate(&c.name), &bold)?;
                }
                enc.workbook = Some(workbook);
            }
        }
        Ok(enc)
    }

    fn row(&mut self, values: &[Option<String>]) -> Result<(), ExportError> {
        match self.format {
            ExportFormat::Csv => {
                self.buf
                    .push_str(&sql_builder::csv_line(values.iter().map(|v| v.as_deref())));
            }
            ExportFormat::Json => {
                if self.count > 0 {
                    self.buf.push(',');
                }
                self.buf.push_str("\n  {");
                for (i, (col, value)) in self.columns.iter().zip(values).enumerate() {
                    if i > 0 {
                        self.buf.push_str(", ");
                    }
                    self.buf
                        .push_str(&Value::String(col.name.clone()).to_string());
                    self.buf.push_str(": ");
                    self.buf
                        .push_str(&sql_builder::json_cell(value.as_deref(), col).to_string());
                }
                self.buf.push('}');
            }
            ExportFormat::Sql => {
                let cols: Vec<&Column> = self.columns.iter().collect();
                self.buf.push_str(&sql_builder::sql_insert_line(
                    &self.insert_target,
                    &cols,
                    values,
                ));
            }
            ExportFormat::Xlsx => {
                let row = u32::try_from(self.count + 1)
                    .map_err(|_| ExportError::Xlsx("too many rows".into()))?;
                let workbook = self.workbook.as_mut().expect("workbook");
                let sheet = workbook.worksheet_from_index(0)?;
                for (i, (col, value)) in self.columns.iter().zip(values).enumerate() {
                    let c = xlsx_col(i)?;
                    match sql_builder::json_cell(value.as_deref(), col) {
                        Value::Null => {}
                        Value::Bool(b) => {
                            sheet.write_boolean(row, c, b)?;
                        }
                        Value::Number(n) if col.kind() == "number" => match n.as_f64() {
                            Some(f) => {
                                sheet.write_number(row, c, f)?;
                            }
                            None => {
                                sheet.write_string(row, c, truncate(&n.to_string()))?;
                            }
                        },
                        _ => {
                            sheet.write_string(
                                row,
                                c,
                                truncate(value.as_deref().unwrap_or_default()),
                            )?;
                        }
                    }
                }
            }
        }
        self.count += 1;
        Ok(())
    }

    /// Bytes ready to send, if enough have accumulated.
    fn ready(&mut self) -> Option<Bytes> {
        (self.buf.len() >= EXPORT_CHUNK_BYTES).then(|| Bytes::from(std::mem::take(&mut self.buf)))
    }

    /// The rest of the file.
    async fn finish(mut self) -> Result<Bytes, ExportError> {
        match self.format {
            ExportFormat::Json => {
                self.buf
                    .push_str(if self.count > 0 { "\n]\n" } else { "]\n" });
                Ok(Bytes::from(self.buf))
            }
            ExportFormat::Xlsx => {
                let mut workbook = self.workbook.take().expect("workbook");
                let bytes = tokio::task::spawn_blocking(move || workbook.save_to_buffer())
                    .await
                    .map_err(|e| ExportError::Xlsx(e.to_string()))??;
                Ok(Bytes::from(bytes))
            }
            _ => Ok(Bytes::from(self.buf)),
        }
    }
}

fn xlsx_col(i: usize) -> Result<u16, ExportError> {
    u16::try_from(i)
        .ok()
        .filter(|&c| c < 16_384)
        .ok_or_else(|| ExportError::Xlsx("Excel sheets hold at most 16,384 columns".into()))
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= XLSX_MAX_STRING_CHARS {
        s.to_string()
    } else {
        s.chars().take(XLSX_MAX_STRING_CHARS).collect()
    }
}

/// Where rows come from once the transaction is open.
enum Rows {
    /// A statement built by `sql_builder` (text columns, bound parameters).
    Table(Sql),
    /// The user's statement wrapped so every column is read as text.
    Text(String),
    /// The user's statement as is; values are decoded from binary.
    Decoded(String),
}

async fn send(
    tx: &mpsc::Sender<Result<Bytes, std::io::Error>>,
    bytes: Bytes,
) -> Result<(), ExportError> {
    if bytes.is_empty() {
        return Ok(());
    }
    tx.send(Ok(bytes))
        .await
        .map_err(|_| ExportError::Disconnected)
}

/// Streams rows into the encoder. Returns the row count and whether the
/// limit cut the export short.
async fn pump(
    conn: &mut PgConnection,
    rows: &Rows,
    mut enc: Encoder,
    limit: u64,
    tx: &mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Result<(u64, bool), ExportError> {
    let mut truncated = false;
    {
        let mut stream = match rows {
            Rows::Table(sql) => {
                let mut q = sqlx::query(&sql.text);
                for p in &sql.params {
                    q = q.bind(p.as_deref());
                }
                q.fetch(&mut *conn)
            }
            Rows::Text(sql) | Rows::Decoded(sql) => sqlx::query(sql).fetch(&mut *conn),
        };
        while let Some(row) = stream.try_next().await? {
            if enc.count >= limit {
                truncated = true;
                break;
            }
            let values = match rows {
                Rows::Decoded(_) => (0..row.len())
                    .map(|i| match decode_cell(&row, i) {
                        Value::Null => None,
                        Value::String(s) => Some(s),
                        other => Some(other.to_string()),
                    })
                    .collect(),
                _ => text_row(&row)?,
            };
            enc.row(&values)?;
            if let Some(bytes) = enc.ready() {
                send(tx, bytes).await?;
            }
        }
    }
    let count = enc.count;
    send(tx, enc.finish().await?).await?;
    Ok((count, truncated))
}

/// Runs the export in the background and records it as a data job.
#[allow(clippy::too_many_arguments)]
fn spawn(
    state: &AppState,
    user_id: Uuid,
    connection_id: Uuid,
    mut conn: PgConnection,
    rows: Rows,
    enc: Encoder,
    limit: u64,
    target: String,
    guard: Option<crate::services::execution::ExecutionGuard>,
) -> mpsc::Receiver<Result<Bytes, std::io::Error>> {
    let (tx, rx) = mpsc::channel(8);
    let db = state.db.clone();
    tokio::spawn(async move {
        let started = Instant::now();
        let started_at = chrono::Utc::now();
        let result = pump(&mut conn, &rows, enc, limit, &tx).await;
        let mut view = JobView::new(
            Uuid::new_v4(),
            JobKind::Export,
            connection_id,
            JobStatus::Completed,
        );
        view.target = Some(target);
        view.started_at = Some(started_at);
        view.finished_at = Some(chrono::Utc::now());
        view.duration_ms = Some(started.elapsed().as_millis() as u64);
        match result {
            Ok((rows, truncated)) => {
                if truncated {
                    // The cursor was not read to the end.
                    drop(conn);
                } else {
                    let _ = conn.execute("ROLLBACK").await;
                    close(conn).await;
                }
                view.rows_processed = rows;
                view.truncated = truncated;
                tracing::info!(%user_id, %connection_id, rows, truncated, "export finished");
            }
            Err(err) => {
                drop(conn);
                tracing::warn!(%user_id, %connection_id, error = %err, "export failed");
                view.status = if matches!(err, ExportError::Disconnected) {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                };
                view.error = Some(err.to_string());
                let _ = tx.send(Err(std::io::Error::other(err))).await;
            }
        }
        drop(guard);
        if let Err(err) = jobs_repo::finish(&db, user_id, &view).await {
            tracing::warn!(error = %err, "failed to record export");
        }
    });
    rx
}

pub struct ExportService<'a> {
    state: &'a AppState,
}

impl<'a> ExportService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    async fn open(&self, user_id: Uuid, connection_id: Uuid) -> AppResult<PgConnection> {
        let record = ConnectionService::new(self.state)
            .get_record(user_id, connection_id)
            .await?;
        DataStudioService::new(self.state)
            .connect(&record, true)
            .await
    }

    async fn begin(&self, conn: &mut PgConnection) -> AppResult<()> {
        conn.execute("BEGIN READ ONLY")
            .await
            .map_err(target_db::query_error)?;
        target_db::set_local_timeouts(conn, self.state.config.query_max_timeout).await
    }

    /// All, filtered or selected rows of a table.
    pub async fn table(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        src: &TableSource,
        format: ExportFormat,
    ) -> AppResult<ExportStream> {
        self.state.query_limiter.check(&user.user_id.to_string())?;
        let mut conn = self.open(user.user_id, connection_id).await?;
        let prepared = async {
            let (table, _) = load_table(&mut conn, &src.schema, &src.table).await?;
            let mut columns: Vec<usize> = match &src.columns {
                Some(names) => names
                    .iter()
                    .map(|n| table.column(n).map(|(i, _)| i))
                    .collect::<AppResult<_>>()?,
                None => (0..table.columns.len()).collect(),
            };
            if format == ExportFormat::Sql {
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
                filter: src.filter.as_ref(),
                sort: &src.sort,
                search: src.search.as_deref(),
            };
            let limit = row_limit(format);
            let sql = sql_builder::build_export(&table, &q, src.keys.as_deref(), &columns, limit)?;
            let cols: Vec<Column> = columns.iter().map(|&i| table.columns[i].clone()).collect();
            let qualified = table.qualified();
            let enc = Encoder::new(format, cols, qualified.clone(), &qualified)
                .map_err(|e| AppError::internal(e.to_string()))?;
            Ok((sql, enc, qualified, limit))
        }
        .await;
        let (sql, enc, qualified, limit) = match prepared {
            Ok(p) => p,
            Err(err) => {
                close(conn).await;
                return Err(err);
            }
        };
        self.begin(&mut conn).await?;
        let body = spawn(
            self.state,
            user.user_id,
            connection_id,
            conn,
            Rows::Table(sql),
            enc,
            limit,
            qualified,
            None,
        );
        Ok(ExportStream {
            filename: format!("{}.{}", safe_file_name(&src.table), format.extension()),
            content_type: format.content_type(),
            body,
        })
    }

    /// The full result of one read-only statement.
    pub async fn query(
        &self,
        user: &AuthUser,
        connection_id: Uuid,
        sql: &str,
        name: Option<&str>,
        format: ExportFormat,
    ) -> AppResult<ExportStream> {
        self.state.query_limiter.check(&user.user_id.to_string())?;
        let guard = self.state.executions.register(
            Uuid::new_v4(),
            user.user_id,
            connection_id,
            self.state.config.max_concurrent_queries_per_user,
        )?;
        let mut conn = self.open(user.user_id, connection_id).await?;
        self.begin(&mut conn).await?;

        // Preparing checks the statement (one statement, valid SQL) and
        // describes its result before anything is streamed.
        let described = (&mut conn).prepare(sql).await.map(|s| {
            s.columns()
                .iter()
                .map(|c| query_column(c.name(), c.type_info()))
                .collect::<Vec<_>>()
        });
        let columns = match described {
            Ok(columns) if columns.is_empty() => {
                close(conn).await;
                return Err(AppError::validation(
                    "source.sql",
                    "the statement returns no rows to export",
                ));
            }
            Ok(columns) => columns,
            Err(err) => {
                close(conn).await;
                return Err(target_db::query_error(err));
            }
        };

        // Read every column as PostgreSQL text when the statement can be
        // used as a subquery; otherwise (SHOW, EXPLAIN, ...) decode values.
        let body_sql = sql.trim_end().trim_end_matches(';').trim_end();
        let aliases: Vec<String> = (1..=columns.len()).map(|i| format!("c{i}")).collect();
        let wrapped = format!(
            "SELECT {} FROM (\n{body_sql}\n) AS querylab_export({})",
            aliases
                .iter()
                .map(|a| format!("CAST({a} AS text)"))
                .collect::<Vec<_>>()
                .join(", "),
            aliases.join(", ")
        );
        conn.execute("SAVEPOINT querylab_export")
            .await
            .map_err(target_db::query_error)?;
        let rows = match (&mut conn).prepare(wrapped.as_str()).await {
            Ok(_) => Rows::Text(wrapped),
            Err(_) => {
                conn.execute("ROLLBACK TO SAVEPOINT querylab_export")
                    .await
                    .map_err(target_db::query_error)?;
                Rows::Decoded(sql.to_string())
            }
        };

        let name = name.unwrap_or("query_result");
        let target = sql_builder::quote_ident(name);
        let enc = Encoder::new(format, columns, target.clone(), "a query")
            .map_err(|e| AppError::internal(e.to_string()))?;
        let limit = row_limit(format);
        let body = spawn(
            self.state,
            user.user_id,
            connection_id,
            conn,
            rows,
            enc,
            limit,
            format!("query → {name}"),
            Some(guard),
        );
        Ok(ExportStream {
            filename: format!("{}.{}", safe_file_name(name), format.extension()),
            content_type: format.content_type(),
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, base: &str, category: char) -> Column {
        Column {
            name: name.into(),
            data_type: base.into(),
            cast_type: base.into(),
            base_type: base.into(),
            category,
            nullable: true,
            has_default: false,
            identity: None,
            generated: false,
            enum_values: vec![],
        }
    }

    fn columns() -> Vec<Column> {
        vec![
            col("id", "int4", 'N'),
            col("name", "text", 'S'),
            col("doc", "jsonb", 'U'),
            col("ok", "bool", 'B'),
        ]
    }

    fn rows() -> Vec<Vec<Option<String>>> {
        vec![
            vec![
                Some("1".into()),
                Some("=SUM(A1)".into()),
                Some("{\"a\": 1}".into()),
                Some("true".into()),
            ],
            vec![Some("2".into()), Some("O'Neil, \"Jr\"".into()), None, None],
        ]
    }

    async fn encode(format: ExportFormat) -> Vec<u8> {
        let mut enc =
            Encoder::new(format, columns(), "public.people".into(), "public.people").unwrap();
        let mut out = Vec::new();
        for r in rows() {
            enc.row(&r).unwrap();
            if let Some(b) = enc.ready() {
                out.extend_from_slice(&b);
            }
        }
        out.extend_from_slice(&enc.finish().await.unwrap());
        out
    }

    // Golden files for each text format.
    #[tokio::test]
    async fn csv_golden() {
        assert_eq!(
            String::from_utf8(encode(ExportFormat::Csv).await).unwrap(),
            "id,name,doc,ok\r\n1,'=SUM(A1),\"{\"\"a\"\": 1}\",true\r\n2,\"O'Neil, \"\"Jr\"\"\",,\r\n"
        );
    }

    #[tokio::test]
    async fn json_golden() {
        assert_eq!(
            String::from_utf8(encode(ExportFormat::Json).await).unwrap(),
            "[\n  {\"id\": 1, \"name\": \"=SUM(A1)\", \"doc\": {\"a\":1}, \"ok\": true},\n  {\"id\": 2, \"name\": \"O'Neil, \\\"Jr\\\"\", \"doc\": null, \"ok\": null}\n]\n"
        );
    }

    #[tokio::test]
    async fn sql_golden() {
        assert_eq!(
            String::from_utf8(encode(ExportFormat::Sql).await).unwrap(),
            "-- Exported from public.people by QueryLab\n\
             INSERT INTO public.people (id, name, doc, ok) VALUES (1, '=SUM(A1)', '{\"a\": 1}', true);\n\
             INSERT INTO public.people (id, name, doc, ok) VALUES (2, 'O''Neil, \"Jr\"', NULL, NULL);\n"
        );
    }

    #[tokio::test]
    async fn xlsx_is_a_zip_workbook() {
        let bytes = encode(ExportFormat::Xlsx).await;
        assert_eq!(&bytes[..2], b"PK");
        assert!(bytes.len() > 1000);
    }

    #[tokio::test]
    async fn empty_json_is_an_empty_array() {
        let enc = Encoder::new(ExportFormat::Json, columns(), String::new(), "x").unwrap();
        assert_eq!(&enc.finish().await.unwrap()[..], b"[]\n");
    }

    #[test]
    fn file_names_and_limits() {
        assert_eq!(safe_file_name("my table/ä"), "my_table__");
        assert_eq!(safe_file_name("..."), "export");
        assert_eq!(row_limit(ExportFormat::Xlsx), MAX_XLSX_ROWS);
        assert_eq!(row_limit(ExportFormat::Csv), MAX_EXPORT_ROWS);
    }
}

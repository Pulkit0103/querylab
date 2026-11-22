//! CSV and JSON imports.
//!
//! Workflow: create an import (reserving memory for the declared size),
//! upload the file in chunks, analyze it (format, columns, inferred types),
//! check a mapping against the target table, then run it.
//!
//! A run is privileged (read-write connection, typed confirmation on
//! production) and happens in one transaction on the target:
//! 1. rows are streamed with `COPY … FROM STDIN` into a temporary staging
//!    table of text columns;
//! 2. on PostgreSQL 16+ every value is checked with `pg_input_is_valid`, so
//!    type errors are found in one pass with their row and line numbers;
//! 3. rows are merged with `INSERT … SELECT` in batches, each under a
//!    savepoint. A failing batch is split in halves until the failing rows
//!    are isolated, which reports constraint violations (unique, foreign
//!    key, check) per row as well.
//!
//! In `all_or_nothing` mode the first invalid row aborts the run and nothing
//! is written; in `skip_invalid` mode invalid rows are reported and left out.

use std::{ops::ControlFlow, sync::Arc, time::Instant};

use axum::body::Bytes;
use futures_util::TryStreamExt;
use sqlx::{Executor, PgConnection, Row};
use uuid::Uuid;

use crate::{
    database::data_jobs as repo,
    errors::{AppError, AppResult},
    models::{
        data_jobs::{
            CheckImportRequest, CheckImportResponse, ColumnMapping, CreateImportRequest,
            CreateImportResponse, DuplicateMode, ErrorMode, ImportPreview, InferredType, JobKind,
            JobStatus, JobView, MappingIssue, ParseOptions, RowError, RunImportRequest, Severity,
            SourceColumn, TargetColumn, MAX_UPLOAD_CHUNK_BYTES,
        },
        library::HistoryKind,
    },
    services::{
        auth::AuthUser,
        connections::ConnectionService,
        data_jobs::{Job, Reservation},
        data_studio::{check_confirmation, close, load_table, DataStudioService},
        import_parse::{self, Item, Layout},
        library::LibraryService,
        sql_builder::{quote_ident, quote_literal, Column, Table},
        target_db,
    },
    state::AppState,
};

/// Rows merged per statement before bisecting failures.
const MERGE_BATCH: i64 = 5000;
/// Bytes per COPY message.
const COPY_CHUNK_BYTES: usize = 1024 * 1024;
const STAGE: &str = "pg_temp.querylab_import_stage";

/// The file of an import, held in memory for the life of the import.
pub struct Upload {
    pub file_name: String,
    pub declared: usize,
    /// Shared with the parser threads once the upload is complete.
    pub data: Arc<Vec<u8>>,
    pub layout: Option<Layout>,
    pub columns: Vec<SourceColumn>,
    _reservation: Reservation,
}

/// What an analyzed import knows: layout, columns, bytes and file name.
type Analyzed = (Layout, Vec<SourceColumn>, Arc<Vec<u8>>, String);

/// A validated run, ready to execute.
struct Plan {
    table: Table,
    /// (source column index, target column index), in mapping order.
    pairs: Vec<(usize, usize)>,
    duplicate_mode: DuplicateMode,
    conflict: Option<Vec<usize>>,
    error_mode: ErrorMode,
    empty_as_null: bool,
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether the column must receive a value from the file.
fn required(c: &Column) -> bool {
    !c.nullable && !c.has_default && c.identity.is_none() && !c.generated
}

/// Matches source columns to target columns by name (ignoring case,
/// spaces and punctuation).
pub fn auto_map(source: &[SourceColumn], table: &Table) -> Vec<ColumnMapping> {
    table
        .columns
        .iter()
        .filter(|c| !c.generated)
        .filter_map(|c| {
            let key = normalize(&c.name);
            source
                .iter()
                .find(|s| normalize(&s.name) == key)
                .map(|s| ColumnMapping {
                    source: s.index,
                    target: c.name.clone(),
                })
        })
        .collect()
}

/// Declared length of `character varying(n)` / `character(n)` columns.
fn declared_length(c: &Column) -> Option<usize> {
    if c.kind() != "text" {
        return None;
    }
    let (_, rest) = c.data_type.split_once('(')?;
    rest.strip_suffix(')')?.parse().ok()
}

fn is_integer_type(c: &Column) -> bool {
    matches!(c.base_type.as_str(), "int2" | "int4" | "int8")
}

/// Why values of the inferred type may not fit the column.
pub fn compatibility(inferred: InferredType, c: &Column) -> Option<String> {
    use InferredType as T;
    let mismatch = match (c.kind(), inferred) {
        (_, T::Unknown) => false,
        ("number", T::Integer) => false,
        ("number", T::Decimal) => {
            if is_integer_type(c) {
                return Some(format!(
                    "{} is an integer column but the values have decimals",
                    c.name
                ));
            }
            false
        }
        ("number", _) => true,
        ("boolean", T::Boolean | T::Integer) => false,
        ("boolean", _) => true,
        ("datetime", T::Integer | T::Decimal | T::Boolean | T::Uuid | T::Json) => true,
        ("uuid", t) => t != T::Uuid,
        ("json", T::Text | T::Date | T::Timestamp | T::Uuid) => true,
        _ => false,
    };
    mismatch.then(|| {
        format!(
            "{} is {} but the values look like {}",
            c.name,
            c.data_type,
            match inferred {
                T::Boolean => "booleans",
                T::Integer => "integers",
                T::Decimal => "decimal numbers",
                T::Date => "dates",
                T::Timestamp => "timestamps",
                T::Uuid => "UUIDs",
                T::Json => "JSON documents",
                _ => "text",
            }
        )
    })
}

/// Problems with a mapping. Errors block a run; warnings are advice.
pub fn mapping_issues(
    source: &[SourceColumn],
    table: &Table,
    mapping: &[ColumnMapping],
) -> Vec<MappingIssue> {
    let mut issues = Vec::new();
    let mut error = |source: Option<usize>, target: Option<&str>, message: String| {
        issues.push(MappingIssue {
            severity: Severity::Error,
            source,
            target: target.map(str::to_string),
            message,
        })
    };
    if !matches!(table.kind.as_str(), "table" | "partitioned_table") {
        error(None, None, "Rows can only be imported into tables.".into());
    }
    if mapping.is_empty() {
        error(None, None, "Map at least one column.".into());
    }
    for m in mapping {
        if m.source >= source.len() {
            error(
                Some(m.source),
                Some(&m.target),
                format!("source column {} does not exist", m.source + 1),
            );
        }
        match table.columns.iter().find(|c| c.name == m.target) {
            None => error(
                Some(m.source),
                Some(&m.target),
                format!("{} has no column \"{}\"", table.qualified(), m.target),
            ),
            Some(c) if c.generated => error(
                Some(m.source),
                Some(&m.target),
                format!(
                    "{} is a generated column and cannot be imported into",
                    c.name
                ),
            ),
            _ => {}
        }
    }
    for c in table.columns.iter().filter(|c| required(c)) {
        if !mapping.iter().any(|m| m.target == c.name) {
            error(
                None,
                Some(&c.name),
                format!("{} is required (NOT NULL without a default)", c.name),
            );
        }
    }

    let mut warning = |source: usize, target: &str, message: String| {
        issues.push(MappingIssue {
            severity: Severity::Warning,
            source: Some(source),
            target: Some(target.to_string()),
            message,
        })
    };
    for m in mapping {
        let (Some(s), Some(c)) = (
            source.get(m.source),
            table.columns.iter().find(|c| c.name == m.target),
        ) else {
            continue;
        };
        if let Some(message) = compatibility(s.inferred_type, c) {
            warning(m.source, &c.name, message);
        }
        if let Some(n) = declared_length(c).filter(|&n| s.max_length > n) {
            warning(
                m.source,
                &c.name,
                format!(
                    "{} holds at most {n} characters; some values have {}",
                    c.name, s.max_length
                ),
            );
        }
        if !c.nullable && s.empty_count > 0 {
            warning(
                m.source,
                &c.name,
                format!(
                    "{} is NOT NULL but {} sampled values are empty",
                    c.name, s.empty_count
                ),
            );
        }
        if c.identity == Some('a') {
            warning(
                m.source,
                &c.name,
                format!(
                    "{} is an identity column: file values replace generated ones (OVERRIDING SYSTEM VALUE)",
                    c.name
                ),
            );
        } else if c.has_default && c.kind() == "number" && table.key.contains(&position(table, c)) {
            warning(
                m.source,
                &c.name,
                format!(
                    "{} has a default such as a sequence; imported values do not advance it",
                    c.name
                ),
            );
        }
    }
    issues
}

fn position(table: &Table, c: &Column) -> usize {
    table
        .columns
        .iter()
        .position(|x| x.name == c.name)
        .unwrap_or(usize::MAX)
}

fn target_column(c: &Column) -> TargetColumn {
    TargetColumn {
        name: c.name.clone(),
        data_type: c.data_type.clone(),
        kind: c.kind(),
        nullable: c.nullable,
        has_default: c.has_default,
        identity: c.identity_label(),
        generated: c.generated,
        enum_values: c.enum_values.clone(),
        required: required(c),
    }
}

/// Column sets of the primary key and valid unique indexes without
/// predicates or expressions (usable as an `ON CONFLICT` target).
pub(crate) async fn unique_keys(conn: &mut PgConnection, oid: i64) -> AppResult<Vec<Vec<String>>> {
    sqlx::query_scalar::<_, Vec<String>>(
        "SELECT ARRAY(SELECT a.attname::text \
                      FROM unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum \
                      WHERE k.ord <= i.indnkeyatts ORDER BY k.ord) \
         FROM pg_index i \
         WHERE i.indrelid = $1::bigint::oid AND i.indisunique AND i.indisvalid \
           AND i.indpred IS NULL AND i.indexprs IS NULL \
         ORDER BY i.indisprimary DESC, i.indnkeyatts, i.indexrelid",
    )
    .bind(oid)
    .fetch_all(conn)
    .await
    .map_err(target_db::query_error)
}

// ---------------------------------------------------------------------------
// Merge SQL
// ---------------------------------------------------------------------------

fn merge_sql(plan: &Plan) -> String {
    let t = &plan.table;
    let targets: Vec<String> = plan
        .pairs
        .iter()
        .map(|&(_, c)| quote_ident(&t.columns[c].name))
        .collect();
    let casts: Vec<String> = plan
        .pairs
        .iter()
        .enumerate()
        .map(|(i, &(_, c))| format!("CAST(s.c{i} AS {})", t.columns[c].cast_type))
        .collect();
    let overriding = if plan
        .pairs
        .iter()
        .any(|&(_, c)| t.columns[c].identity == Some('a'))
    {
        " OVERRIDING SYSTEM VALUE"
    } else {
        ""
    };
    let key = |cols: &[usize]| {
        cols.iter()
            .map(|&c| quote_ident(&t.columns[c].name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let conflict = match (plan.duplicate_mode, &plan.conflict) {
        (DuplicateMode::Error, _) => String::new(),
        (DuplicateMode::Skip, None) => " ON CONFLICT DO NOTHING".into(),
        (DuplicateMode::Skip, Some(cols)) => format!(" ON CONFLICT ({}) DO NOTHING", key(cols)),
        (DuplicateMode::Update, cols) => {
            let cols = cols.as_deref().unwrap_or_default();
            let updates: Vec<String> = plan
                .pairs
                .iter()
                .filter(|(_, c)| !cols.contains(c))
                .map(|&(_, c)| {
                    let name = quote_ident(&t.columns[c].name);
                    format!("{name} = EXCLUDED.{name}")
                })
                .collect();
            if updates.is_empty() {
                format!(" ON CONFLICT ({}) DO NOTHING", key(cols))
            } else {
                format!(
                    " ON CONFLICT ({}) DO UPDATE SET {}",
                    key(cols),
                    updates.join(", ")
                )
            }
        }
    };
    format!(
        "WITH ins AS (INSERT INTO {} ({}){overriding} SELECT {} FROM {STAGE} s \
         WHERE s.row_no BETWEEN $1 AND $2 ORDER BY s.row_no{conflict} RETURNING (xmax = 0) AS inserted) \
         SELECT count(*) FILTER (WHERE inserted)::bigint, count(*) FILTER (WHERE NOT inserted)::bigint FROM ins",
        t.qualified(),
        targets.join(", "),
        casts.join(", ")
    )
}

/// Per-row validity conditions (PostgreSQL 16+): one problem expression and
/// one condition per mapped column, with the cast types as parameters.
fn validation_sql(plan: &Plan) -> (String, String, Vec<String>) {
    let t = &plan.table;
    let mut problems = Vec::new();
    let mut conditions = Vec::new();
    let mut types = Vec::new();
    for (i, &(_, c)) in plan.pairs.iter().enumerate() {
        let col = &t.columns[c];
        types.push(col.cast_type.clone());
        let p = types.len();
        let label = quote_literal(&col.name);
        problems.push(format!(
            "CASE WHEN s.c{i} IS NOT NULL AND NOT pg_input_is_valid(s.c{i}, ${p}) \
                  THEN {label} || '\u{1f}' || (pg_input_error_info(s.c{i}, ${p})).message END"
        ));
        conditions.push(format!(
            "(s.c{i} IS NOT NULL AND NOT pg_input_is_valid(s.c{i}, ${p}))"
        ));
        if !col.nullable {
            problems.push(format!(
                "CASE WHEN s.c{i} IS NULL THEN {label} || '\u{1f}' || 'null value in a NOT NULL column' END"
            ));
            conditions.push(format!("s.c{i} IS NULL"));
        }
    }
    let condition = conditions.join(" OR ");
    let select = format!(
        "SELECT s.row_no, s.line, array_remove(ARRAY[{}]::text[], NULL) \
         FROM {STAGE} s WHERE {condition} ORDER BY s.row_no",
        problems.join(", ")
    );
    (select, condition, types)
}

/// Errors that end the whole run rather than one row: cancellation and
/// timeouts, lost connections, resource exhaustion, internal errors.
fn is_fatal(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => db.code().is_some_and(|c| {
            ["57", "08", "53", "58", "XX", "40"]
                .iter()
                .any(|p| c.starts_with(p))
        }),
        _ => true,
    }
}

fn row_message(err: sqlx::Error) -> String {
    match target_db::query_error(err) {
        AppError::Sql(info) => match info.detail {
            Some(detail) => format!("{} ({detail})", info.message),
            None => info.message,
        },
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct ImportService<'a> {
    state: &'a AppState,
}

impl<'a> ImportService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    fn job(&self, user_id: Uuid, id: Uuid) -> AppResult<Arc<Job>> {
        self.state
            .jobs
            .get(user_id, id)
            .filter(|j| j.kind == JobKind::Import)
            .ok_or(AppError::NotFound("import"))
    }

    pub async fn create(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        req: &CreateImportRequest,
    ) -> AppResult<CreateImportResponse> {
        ConnectionService::new(self.state)
            .get_record(user_id, connection_id)
            .await?;
        let max = self.state.config.import_max_bytes;
        let size = usize::try_from(req.size).unwrap_or(usize::MAX);
        if size > max {
            return Err(AppError::validation(
                "size",
                format!("files up to {} MiB can be imported", max / (1024 * 1024)),
            ));
        }
        let reservation = self.state.jobs.reserve(size)?;
        let id = Uuid::new_v4();
        let mut view = JobView::new(id, JobKind::Import, connection_id, JobStatus::Uploading);
        view.file_name = Some(req.file_name.clone());
        view.bytes_total = req.size;
        let upload = Upload {
            file_name: req.file_name.clone(),
            declared: size,
            data: Arc::new(Vec::with_capacity(size)),
            layout: None,
            columns: Vec::new(),
            _reservation: reservation,
        };
        self.state
            .jobs
            .create(user_id, connection_id, view, Some(upload))?;
        Ok(CreateImportResponse {
            import_id: id,
            max_chunk_bytes: MAX_UPLOAD_CHUNK_BYTES,
        })
    }

    /// Appends a chunk at `offset`, which must equal the bytes received so
    /// far (a retried chunk that already arrived is acknowledged).
    pub fn upload(&self, user_id: Uuid, id: Uuid, offset: u64, chunk: Bytes) -> AppResult<JobView> {
        let job = self.job(user_id, id)?;
        if job.status() != JobStatus::Uploading {
            return Err(AppError::Conflict(
                "the file has already been uploaded".into(),
            ));
        }
        if chunk.len() > MAX_UPLOAD_CHUNK_BYTES {
            return Err(AppError::PayloadTooLarge);
        }
        let received = {
            let mut guard = job.upload();
            let upload = guard.as_mut().ok_or(AppError::NotFound("import"))?;
            let len = upload.data.len() as u64;
            if offset + chunk.len() as u64 == len {
                // Retry of the last chunk.
                return Ok(job.view());
            }
            if offset != len {
                return Err(AppError::Conflict(format!(
                    "expected the chunk at offset {len}, got {offset}"
                )));
            }
            if upload.data.len() + chunk.len() > upload.declared {
                return Err(AppError::validation(
                    "size",
                    "the upload is larger than the declared file size",
                ));
            }
            let data = Arc::get_mut(&mut upload.data)
                .ok_or_else(|| AppError::Conflict("the file is in use".into()))?;
            data.extend_from_slice(&chunk);
            (data.len(), upload.declared)
        };
        Ok(job.update(|v| {
            v.bytes_received = received.0 as u64;
            if received.0 == received.1 {
                v.status = JobStatus::Ready;
            }
            v.clone()
        }))
    }

    /// Detects the layout and infers column types; can be repeated with
    /// different options until the import runs.
    pub async fn analyze(
        &self,
        user_id: Uuid,
        id: Uuid,
        opts: ParseOptions,
    ) -> AppResult<ImportPreview> {
        let job = self.job(user_id, id)?;
        if job.status() != JobStatus::Ready {
            return Err(AppError::Conflict(match job.status() {
                JobStatus::Uploading => "the file has not been uploaded completely".into(),
                _ => "this import has already run".into(),
            }));
        }
        let data = job
            .upload()
            .as_ref()
            .map(|u| Arc::clone(&u.data))
            .ok_or(AppError::NotFound("import"))?;
        let (layout, mut preview) =
            tokio::task::spawn_blocking(move || import_parse::analyze(&data, &opts))
                .await
                .map_err(AppError::internal)??;
        preview.import_id = id;
        if let Some(upload) = job.upload().as_mut() {
            upload.layout = Some(layout);
            upload.columns = preview.columns.clone();
        }
        job.update(|v| v.rows_total = Some(preview.total_rows));
        Ok(preview)
    }

    fn analyzed(&self, job: &Job) -> AppResult<Analyzed> {
        match job.status() {
            JobStatus::Running => {
                return Err(AppError::Conflict("this import is already running".into()))
            }
            s if s.is_finished() => {
                return Err(AppError::Conflict("this import has already run".into()))
            }
            _ => {}
        }
        let guard = job.upload();
        let upload = guard.as_ref().ok_or(AppError::NotFound("import"))?;
        let layout = upload.layout.clone().ok_or_else(|| {
            AppError::Conflict("analyze the file before mapping its columns".into())
        })?;
        Ok((
            layout,
            upload.columns.clone(),
            Arc::clone(&upload.data),
            upload.file_name.clone(),
        ))
    }

    /// The target's columns and keys, a suggested or checked mapping and
    /// its problems.
    pub async fn check(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &CheckImportRequest,
    ) -> AppResult<CheckImportResponse> {
        let job = self.job(user_id, id)?;
        let (_, source, _, _) = self.analyzed(&job)?;
        let record = ConnectionService::new(self.state)
            .get_record(user_id, job.connection_id)
            .await?;
        let mut conn = DataStudioService::new(self.state)
            .connect(&record, true)
            .await?;
        let loaded = load_table(&mut conn, &req.schema, &req.table).await;
        let keys = match &loaded {
            Ok((_, oid)) => unique_keys(&mut conn, *oid).await,
            Err(_) => Ok(Vec::new()),
        };
        close(conn).await;
        let (table, _) = loaded?;
        let mapping = req
            .mapping
            .clone()
            .unwrap_or_else(|| auto_map(&source, &table));
        Ok(CheckImportResponse {
            columns: table.columns.iter().map(target_column).collect(),
            issues: mapping_issues(&source, &table, &mapping),
            mapping,
            unique_keys: keys?,
            can_insert: table.can_insert,
            can_update: table.can_update,
            connection_read_only: record.read_only,
        })
    }

    pub async fn run(
        &self,
        user: &AuthUser,
        id: Uuid,
        req: &RunImportRequest,
    ) -> AppResult<JobView> {
        let job = self.job(user.user_id, id)?;
        let (layout, source, data, file_name) = self.analyzed(&job)?;
        let record = ConnectionService::new(self.state)
            .get_record(user.user_id, job.connection_id)
            .await?;
        if record.read_only {
            return Err(AppError::Forbidden(
                "This connection is read-only. Enable read-write mode in the connection settings to import data."
                    .into(),
            ));
        }
        check_confirmation(&record, req.confirm.as_deref())?;

        let mut conn = DataStudioService::new(self.state)
            .connect(&record, false)
            .await?;
        let plan = match self.plan(&mut conn, &source, req).await {
            Ok(plan) => plan,
            Err(err) => {
                close(conn).await;
                return Err(err);
            }
        };
        let target = plan.table.qualified();
        let view = match job.start(Some(target)) {
            Ok(view) => view,
            Err(err) => {
                close(conn).await;
                return Err(err);
            }
        };
        if let Err(err) = repo::insert_running(&self.state.db, user.user_id, &view).await {
            tracing::warn!(error = %err, "failed to record data job");
        }
        let state = self.state.clone();
        let user_id = user.user_id;
        crate::services::audit::spawn_in_request(async move {
            run_import(state, job, conn, plan, layout, data, file_name, user_id).await;
        });
        Ok(view)
    }

    async fn plan(
        &self,
        conn: &mut PgConnection,
        source: &[SourceColumn],
        req: &RunImportRequest,
    ) -> AppResult<Plan> {
        let (table, oid) = load_table(conn, &req.schema, &req.table).await?;
        let issues = mapping_issues(source, &table, &req.mapping);
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| crate::errors::FieldError {
                field: "mapping".into(),
                message: i.message.clone(),
            })
            .collect();
        if !errors.is_empty() {
            return Err(AppError::Validation(errors));
        }
        let qualified = table.qualified();
        if !table.can_insert {
            return Err(AppError::Forbidden(format!(
                "The connection's role does not have INSERT privilege on {qualified}."
            )));
        }
        if req.duplicate_mode == DuplicateMode::Update && !table.can_update {
            return Err(AppError::Forbidden(format!(
                "The connection's role does not have UPDATE privilege on {qualified}."
            )));
        }
        let conflict = match (&req.conflict_columns, req.duplicate_mode) {
            (_, DuplicateMode::Error) | (None, _) => None,
            (Some(cols), _) => {
                let keys = unique_keys(conn, oid).await?;
                let mut wanted = cols.clone();
                wanted.sort();
                let matches = keys.iter().any(|k| {
                    let mut k = k.clone();
                    k.sort();
                    k == wanted
                });
                if !matches {
                    return Err(AppError::validation(
                        "conflict_columns",
                        "must be the primary key or the columns of a unique index",
                    ));
                }
                Some(
                    cols.iter()
                        .map(|c| table.column(c).map(|(i, _)| i))
                        .collect::<AppResult<Vec<_>>>()?,
                )
            }
        };
        let pairs = req
            .mapping
            .iter()
            .map(|m| table.column(&m.target).map(|(i, _)| (m.source, i)))
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Plan {
            table,
            pairs,
            duplicate_mode: req.duplicate_mode,
            conflict,
            error_mode: req.error_mode,
            empty_as_null: req.empty_as_null,
        })
    }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

enum Outcome {
    Committed,
    /// Rolled back on purpose (invalid rows in all-or-nothing mode).
    Rejected(String),
    Cancelled,
    Failed(String),
}

#[allow(clippy::too_many_arguments)]
async fn run_import(
    state: AppState,
    job: Arc<Job>,
    mut conn: PgConnection,
    plan: Plan,
    layout: Layout,
    data: Arc<Vec<u8>>,
    file_name: String,
    user_id: Uuid,
) {
    let started = Instant::now();
    let outcome = match import(&state, &job, &mut conn, &plan, &layout, data).await {
        Ok(outcome) => outcome,
        Err(err) => Outcome::Failed(err.to_string()),
    };
    let (status, error) = match outcome {
        Outcome::Committed => {
            close(conn).await;
            (JobStatus::Completed, None)
        }
        other => {
            // Dropping the connection rolls the transaction back.
            drop(conn);
            job.update(|v| {
                v.committed = false;
                v.rows_inserted = 0;
                v.rows_updated = 0;
            });
            match other {
                Outcome::Rejected(m) => (JobStatus::Failed, Some(m)),
                Outcome::Cancelled => (
                    JobStatus::Cancelled,
                    Some("the import was cancelled".into()),
                ),
                Outcome::Failed(m) => (JobStatus::Failed, Some(m)),
                Outcome::Committed => unreachable!(),
            }
        }
    };
    // Parse errors are found before value and constraint errors; report
    // them all in file order.
    job.update(|v| v.errors.sort_by_key(|e| e.row));
    let view = job.finish(&state.db, status, error).await;
    let summary = format!(
        "-- Import {file_name} into {}: {} inserted, {} updated, {} skipped, {} failed{}",
        plan.table.qualified(),
        view.rows_inserted,
        view.rows_updated,
        view.rows_skipped,
        view.rows_failed,
        if view.committed {
            ""
        } else {
            " (nothing committed)"
        }
    );
    LibraryService::new(&state)
        .record_script(
            user_id,
            job.connection_id,
            HistoryKind::Import,
            &summary,
            started.elapsed().as_millis() as u64,
            view.committed
                .then_some((view.rows_inserted + view.rows_updated) as i64),
            view.error.as_deref(),
        )
        .await;
    tracing::info!(
        %user_id, import_id = %job.id, status = status.as_str(),
        inserted = view.rows_inserted, failed = view.rows_failed, "import finished"
    );
}

/// Stages the file through COPY; returns rows staged.
async fn stage(
    job: &Job,
    conn: &mut PgConnection,
    plan: &Plan,
    layout: &Layout,
    data: Arc<Vec<u8>>,
) -> AppResult<u64> {
    let columns: Vec<String> = (0..plan.pairs.len())
        .map(|i| format!("c{i} text"))
        .collect();
    conn.execute(
        format!(
            "CREATE TEMP TABLE querylab_import_stage (row_no bigint NOT NULL, line bigint, {}) ON COMMIT DROP",
            columns.join(", ")
        )
        .as_str(),
    )
    .await
    .map_err(target_db::query_error)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(4);
    let sources: Vec<usize> = plan.pairs.iter().map(|&(s, _)| s).collect();
    let layout = layout.clone();
    let empty_as_null = plan.empty_as_null;
    let parser = tokio::task::spawn_blocking(move || {
        let mut errors: Vec<RowError> = Vec::new();
        let mut buf = Vec::with_capacity(COPY_CHUNK_BYTES + 64 * 1024);
        let mut staged = 0u64;
        let result = import_parse::for_each(&data, &layout, |item| {
            match item {
                Item::Error(e) => errors.push(e),
                Item::Record(r) => {
                    let nul = sources
                        .iter()
                        .find(|&&s| r.values[s].as_deref().is_some_and(|v| v.contains('\0')));
                    if let Some(&s) = nul {
                        errors.push(RowError {
                            row: r.row,
                            line: r.line,
                            column: Some(layout.columns[s].clone()),
                            message: "the value contains a NUL character".into(),
                        });
                        return ControlFlow::Continue(());
                    }
                    buf.extend_from_slice(r.row.to_string().as_bytes());
                    buf.push(b'\t');
                    match r.line {
                        Some(l) => buf.extend_from_slice(l.to_string().as_bytes()),
                        None => buf.extend_from_slice(b"\\N"),
                    }
                    for &s in &sources {
                        buf.push(b'\t');
                        let v = r.values[s]
                            .as_deref()
                            .filter(|v| !(empty_as_null && v.is_empty()));
                        import_parse::copy_value(&mut buf, v);
                    }
                    buf.push(b'\n');
                    staged += 1;
                    if buf.len() >= COPY_CHUNK_BYTES {
                        let chunk = std::mem::replace(
                            &mut buf,
                            Vec::with_capacity(COPY_CHUNK_BYTES + 64 * 1024),
                        );
                        if tx.blocking_send(chunk).is_err() {
                            return ControlFlow::Break(());
                        }
                    }
                }
            }
            ControlFlow::Continue(())
        });
        if !buf.is_empty() {
            let _ = tx.blocking_send(buf);
        }
        result.map(|()| (errors, staged))
    });

    let copy_result: Result<(), sqlx::Error> = async {
        let mut copy = conn
            .copy_in_raw("COPY pg_temp.querylab_import_stage FROM STDIN")
            .await?;
        while let Some(chunk) = rx.recv().await {
            if job.is_cancelled() {
                copy.abort("cancelled").await?;
                return Ok(());
            }
            copy.send(chunk).await?;
        }
        copy.finish().await?;
        Ok(())
    }
    .await;
    // Unblocks the parser if COPY stopped early.
    drop(rx);
    let parsed = parser.await.map_err(AppError::internal)?;
    copy_result.map_err(target_db::query_error)?;
    let (errors, staged) = parsed?;
    job.update(|v| {
        for e in errors {
            v.push_error(e);
        }
    });
    Ok(staged)
}

async fn import(
    state: &AppState,
    job: &Job,
    conn: &mut PgConnection,
    plan: &Plan,
    layout: &Layout,
    data: Arc<Vec<u8>>,
) -> AppResult<Outcome> {
    conn.execute("BEGIN")
        .await
        .map_err(target_db::query_error)?;
    target_db::set_local_timeouts(conn, state.config.query_max_timeout).await?;
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&mut *conn)
        .await
        .map_err(target_db::query_error)?;

    job.update(|v| v.phase = Some("Reading the file".into()));
    let staged = stage(job, conn, plan, layout, data).await?;
    if job.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }

    let mut invalid = 0u64;
    if version >= 160_000 {
        job.update(|v| v.phase = Some("Checking values".into()));
        let (select, condition, types) = validation_sql(plan);
        let mut q = sqlx::query(&select);
        for t in &types {
            q = q.bind(t);
        }
        let mut rows = q.fetch(&mut *conn);
        while let Some(row) = rows.try_next().await.map_err(target_db::query_error)? {
            let row_no: i64 = row.try_get(0).map_err(target_db::query_error)?;
            let line: Option<i64> = row.try_get(1).map_err(target_db::query_error)?;
            let problems: Vec<String> = row.try_get(2).map_err(target_db::query_error)?;
            invalid += 1;
            job.update(|v| {
                for p in problems.iter().take(1) {
                    let (column, message) = p.split_once('\u{1f}').unwrap_or(("", p));
                    v.push_error(RowError {
                        row: row_no as u64,
                        line: line.map(|l| l as u64),
                        column: (!column.is_empty()).then(|| column.to_string()),
                        message: if problems.len() > 1 {
                            format!("{message} (and {} more problems)", problems.len() - 1)
                        } else {
                            message.to_string()
                        },
                    });
                }
            });
        }
        drop(rows);
        if invalid > 0 && plan.error_mode == ErrorMode::SkipInvalid {
            let delete = format!("DELETE FROM {STAGE} s WHERE {condition}");
            let mut q = sqlx::query(&delete);
            for t in &types {
                q = q.bind(t);
            }
            q.execute(&mut *conn)
                .await
                .map_err(target_db::query_error)?;
        }
    }

    let failed_before_merge = job.update(|v| v.rows_failed);
    if failed_before_merge > 0 && plan.error_mode == ErrorMode::AllOrNothing {
        return Ok(Outcome::Rejected(format!(
            "{failed_before_merge} rows are invalid; nothing was imported"
        )));
    }

    // Merge.
    job.update(|v| v.phase = Some(format!("Writing to {}", plan.table.qualified())));
    let range: (Option<i64>, Option<i64>) =
        sqlx::query_as(&format!("SELECT min(row_no), max(row_no) FROM {STAGE}"))
            .fetch_one(&mut *conn)
            .await
            .map_err(target_db::query_error)?;
    let sql = merge_sql(plan);
    let mut merge_failures = 0u64;
    if let (Some(first), Some(last)) = range {
        let mut lo = first;
        while lo <= last {
            let hi = (lo + MERGE_BATCH - 1).min(last);
            let mut stack = vec![(lo, hi)];
            while let Some((a, b)) = stack.pop() {
                if job.is_cancelled() {
                    return Ok(Outcome::Cancelled);
                }
                conn.execute("SAVEPOINT querylab_batch")
                    .await
                    .map_err(target_db::query_error)?;
                let result: Result<(i64, i64), sqlx::Error> = sqlx::query_as(&sql)
                    .bind(a)
                    .bind(b)
                    .fetch_one(&mut *conn)
                    .await;
                match result {
                    Ok((inserted, updated)) => {
                        conn.execute("RELEASE SAVEPOINT querylab_batch")
                            .await
                            .map_err(target_db::query_error)?;
                        job.update(|v| {
                            v.rows_inserted += inserted as u64;
                            v.rows_updated += updated as u64;
                            v.rows_processed += (b - a + 1) as u64;
                        });
                    }
                    Err(err) if is_fatal(&err) => return Err(target_db::query_error(err)),
                    Err(err) => {
                        conn.execute("ROLLBACK TO SAVEPOINT querylab_batch")
                            .await
                            .map_err(target_db::query_error)?;
                        if a < b {
                            let mid = a + (b - a) / 2;
                            stack.push((mid + 1, b));
                            stack.push((a, mid));
                            continue;
                        }
                        let line: Option<Option<i64>> = sqlx::query_scalar(&format!(
                            "SELECT line FROM {STAGE} WHERE row_no = $1"
                        ))
                        .bind(a)
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(target_db::query_error)?;
                        let Some(line) = line else {
                            // Row removed during validation: nothing to report.
                            job.update(|v| v.rows_processed += 1);
                            continue;
                        };
                        let message = row_message(err);
                        merge_failures += 1;
                        job.update(|v| {
                            v.rows_processed += 1;
                            v.push_error(RowError {
                                row: a as u64,
                                line: line.map(|l| l as u64),
                                column: None,
                                message: message.clone(),
                            });
                        });
                        if plan.error_mode == ErrorMode::AllOrNothing {
                            return Ok(Outcome::Rejected(format!(
                                "row {a} failed: {message}; nothing was imported"
                            )));
                        }
                    }
                }
            }
            lo = hi + 1;
        }
    }

    job.update(|v| v.phase = Some("Committing".into()));
    conn.execute("COMMIT")
        .await
        .map_err(target_db::query_error)?;
    let valid = staged.saturating_sub(invalid);
    job.update(|v| {
        v.committed = true;
        v.rows_processed = v.rows_total.unwrap_or(v.rows_processed);
        v.rows_skipped = valid
            .saturating_sub(v.rows_inserted + v.rows_updated)
            .saturating_sub(merge_failures);
    });
    Ok(Outcome::Committed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, data_type: &str, base: &str, category: char) -> Column {
        Column {
            name: name.into(),
            data_type: data_type.into(),
            cast_type: data_type.split('(').next().unwrap_or(data_type).to_string(),
            base_type: base.into(),
            category,
            nullable: true,
            has_default: false,
            identity: None,
            generated: false,
            enum_values: vec![],
        }
    }

    fn table() -> Table {
        let mut id = col("id", "integer", "int4", 'N');
        id.nullable = false;
        id.has_default = true;
        let mut email = col("Email Address", "character varying(10)", "varchar", 'S');
        email.nullable = false;
        let mut total = col("total", "numeric", "numeric", 'N');
        total.generated = true;
        Table {
            schema: "public".into(),
            name: "people".into(),
            kind: "table".into(),
            columns: vec![
                id,
                email,
                col("age", "integer", "int4", 'N'),
                col("born", "date", "date", 'D'),
                total,
            ],
            key: vec![0],
            key_kind: None,
            can_insert: true,
            can_update: true,
            can_delete: true,
        }
    }

    fn source(names: &[(&str, InferredType, usize)]) -> Vec<SourceColumn> {
        names
            .iter()
            .enumerate()
            .map(|(index, (name, t, len))| SourceColumn {
                index,
                name: name.to_string(),
                inferred_type: *t,
                empty_count: 0,
                max_length: *len,
            })
            .collect()
    }

    #[test]
    fn auto_mapping_ignores_case_and_punctuation() {
        let src = source(&[
            ("EMAIL_ADDRESS", InferredType::Text, 5),
            ("Age", InferredType::Integer, 2),
            ("total", InferredType::Decimal, 3),
            ("unrelated", InferredType::Text, 1),
        ]);
        let mapping = auto_map(&src, &table());
        assert_eq!(
            mapping,
            vec![
                ColumnMapping {
                    source: 0,
                    target: "Email Address".into()
                },
                ColumnMapping {
                    source: 1,
                    target: "age".into()
                },
            ]
        );
    }

    #[test]
    fn mapping_issues_report_errors_and_warnings() {
        let src = source(&[
            ("id", InferredType::Integer, 3),
            ("age", InferredType::Decimal, 4),
            ("born", InferredType::Integer, 4),
            ("mail", InferredType::Text, 25),
        ]);
        let t = table();
        let mapping = vec![
            ColumnMapping {
                source: 0,
                target: "id".into(),
            },
            ColumnMapping {
                source: 1,
                target: "age".into(),
            },
            ColumnMapping {
                source: 2,
                target: "born".into(),
            },
            ColumnMapping {
                source: 9,
                target: "total".into(),
            },
        ];
        let issues = mapping_issues(&src, &t, &mapping);
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.message.as_str())
            .collect();
        assert_eq!(
            errors,
            [
                "source column 10 does not exist",
                "total is a generated column and cannot be imported into",
                "Email Address is required (NOT NULL without a default)",
            ]
        );
        let warnings: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .map(|i| i.message.as_str())
            .collect();
        assert_eq!(
            warnings,
            [
                "id has a default such as a sequence; imported values do not advance it",
                "age is an integer column but the values have decimals",
                "born is date but the values look like integers",
            ]
        );

        let ok = mapping_issues(
            &src,
            &t,
            &[ColumnMapping {
                source: 3,
                target: "Email Address".into(),
            }],
        );
        assert_eq!(ok.len(), 1);
        assert_eq!(
            ok[0].message,
            "Email Address holds at most 10 characters; some values have 25"
        );
    }

    #[test]
    fn merge_statement_handles_each_duplicate_mode() {
        let mut t = table();
        t.columns[0].identity = Some('a');
        let mut plan = Plan {
            table: t,
            pairs: vec![(0, 0), (1, 1), (2, 2)],
            duplicate_mode: DuplicateMode::Error,
            conflict: None,
            error_mode: ErrorMode::AllOrNothing,
            empty_as_null: true,
        };
        let sql = merge_sql(&plan);
        assert!(sql.starts_with(
            "WITH ins AS (INSERT INTO public.people (id, \"Email Address\", age) OVERRIDING SYSTEM VALUE \
             SELECT CAST(s.c0 AS integer), CAST(s.c1 AS character varying), CAST(s.c2 AS integer) \
             FROM pg_temp.querylab_import_stage s WHERE s.row_no BETWEEN $1 AND $2 ORDER BY s.row_no RETURNING"
        ), "{sql}");
        plan.duplicate_mode = DuplicateMode::Skip;
        assert!(merge_sql(&plan).contains("ORDER BY s.row_no ON CONFLICT DO NOTHING RETURNING"));
        plan.duplicate_mode = DuplicateMode::Update;
        plan.conflict = Some(vec![0]);
        assert!(merge_sql(&plan).contains(
            "ON CONFLICT (id) DO UPDATE SET \"Email Address\" = EXCLUDED.\"Email Address\", age = EXCLUDED.age RETURNING"
        ));
        plan.pairs = vec![(0, 0)];
        assert!(merge_sql(&plan).contains("ON CONFLICT (id) DO NOTHING"));
    }

    #[test]
    fn validation_checks_types_and_not_null() {
        let plan = Plan {
            table: table(),
            pairs: vec![(0, 1), (1, 2)],
            duplicate_mode: DuplicateMode::Error,
            conflict: None,
            error_mode: ErrorMode::SkipInvalid,
            empty_as_null: true,
        };
        let (select, condition, types) = validation_sql(&plan);
        assert_eq!(types, ["character varying", "integer"]);
        assert_eq!(
            condition,
            "(s.c0 IS NOT NULL AND NOT pg_input_is_valid(s.c0, $1)) OR s.c0 IS NULL OR \
             (s.c1 IS NOT NULL AND NOT pg_input_is_valid(s.c1, $2))"
        );
        assert!(select.contains("'Email Address' || '\u{1f}' || 'null value in a NOT NULL column'"));
    }
}

//! Imports, exports and mock data runs (Phase 9).

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::{
        data::{ExportFormat, TableSource, MAX_CONFIRM_CHARS},
        schema::validate_identifier,
    },
    utils::validation::{Validate, Validator},
};

/// Largest chunk accepted by one upload request. Small enough for any
/// reverse proxy in front of the API.
pub const MAX_UPLOAD_CHUNK_BYTES: usize = 4 * 1024 * 1024;
/// Errors kept in a report; counts are always exact.
pub const MAX_REPORTED_ERRORS: usize = 1000;
/// Rows sampled for type inference and previews.
pub const SAMPLE_ROWS: usize = 1000;
/// Rows returned in an import preview.
pub const PREVIEW_ROWS: usize = 20;
/// Mock data limits.
pub const MAX_MOCK_ROWS_PER_TABLE: u64 = 100_000;
pub const MAX_MOCK_ROWS_PER_RUN: u64 = 1_000_000;
pub const MAX_MOCK_TABLES: usize = 50;
const MAX_SQL_BYTES: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    MockData,
    Export,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Import => "import",
            JobKind::MockData => "mock_data",
            JobKind::Export => "export",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// An import whose file is still being uploaded.
    Uploading,
    /// Uploaded (and possibly analyzed), waiting to run.
    Ready,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Uploading => "uploading",
            JobStatus::Ready => "ready",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_finished(self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

/// A rejected row: `row` is the 1-based data row (header excluded); `line`
/// the 1-based line in the file where the row starts, when the format has
/// lines (CSV, NDJSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RowError {
    pub row: u64,
    pub line: Option<u64>,
    pub column: Option<String>,
    pub message: String,
}

/// Rows written to one table by a mock data run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableCount {
    pub table: String,
    pub requested: u64,
    pub inserted: u64,
}

/// Live state of a job, also stored as the job's report when it finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobView {
    pub id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    pub connection_id: Option<Uuid>,
    pub target: Option<String>,
    /// What the job is doing now, e.g. "Staging rows" or "Writing sales.orders".
    pub phase: Option<String>,
    pub file_name: Option<String>,
    pub bytes_total: u64,
    pub bytes_received: u64,
    pub rows_total: Option<u64>,
    pub rows_processed: u64,
    pub rows_inserted: u64,
    pub rows_updated: u64,
    /// Rows left out because of a duplicate key (`skip` mode or mock data).
    pub rows_skipped: u64,
    pub rows_failed: u64,
    pub errors: Vec<RowError>,
    pub tables: Vec<TableCount>,
    /// Whether the changes were committed.
    pub committed: bool,
    /// An export stopped at its row limit.
    #[serde(default)]
    pub truncated: bool,
    /// Why the job failed as a whole.
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

impl JobView {
    pub fn new(id: Uuid, kind: JobKind, connection_id: Uuid, status: JobStatus) -> Self {
        Self {
            id,
            kind,
            status,
            connection_id: Some(connection_id),
            target: None,
            phase: None,
            file_name: None,
            bytes_total: 0,
            bytes_received: 0,
            rows_total: None,
            rows_processed: 0,
            rows_inserted: 0,
            rows_updated: 0,
            rows_skipped: 0,
            rows_failed: 0,
            errors: Vec::new(),
            tables: Vec::new(),
            committed: false,
            truncated: false,
            error: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            duration_ms: None,
        }
    }

    /// Records a rejected row; the count is exact, the list is capped.
    pub fn push_error(&mut self, error: RowError) {
        self.rows_failed += 1;
        if self.errors.len() < MAX_REPORTED_ERRORS {
            self.errors.push(error);
        }
    }
}

/// A finished or interrupted job as stored in `data_jobs`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct JobRecord {
    pub id: Uuid,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub kind: String,
    pub status: String,
    pub target: Option<String>,
    pub rows_processed: i64,
    pub rows_failed: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct JobsQuery {
    pub kind: Option<JobKind>,
    pub limit: Option<i64>,
}

// ---------------------------------------------------------------------------
// Imports
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateImportRequest {
    pub file_name: String,
    /// Exact size of the file that will be uploaded.
    pub size: u64,
}

impl Validate for CreateImportRequest {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .length("file_name", &self.file_name, 1, 255)
            .no_control_chars("file_name", &self.file_name)
            .check(self.size > 0, "size", "the file is empty")
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct CreateImportResponse {
    pub import_id: Uuid,
    pub max_chunk_bytes: usize,
}

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// Byte offset of this chunk; must equal the bytes received so far.
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    #[default]
    Auto,
    Csv,
    Json,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParseOptions {
    #[serde(default)]
    pub format: FileFormat,
    /// CSV delimiter; detected from the first line when absent.
    #[serde(default)]
    pub delimiter: Option<char>,
    /// The first CSV row holds column names.
    #[serde(default = "default_true")]
    pub has_header: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            format: FileFormat::Auto,
            delimiter: None,
            has_header: true,
        }
    }
}

fn default_true() -> bool {
    true
}

impl Validate for ParseOptions {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(d) = self.delimiter {
            v.check(
                d.is_ascii() && !matches!(d, '"' | '\n' | '\r'),
                "delimiter",
                "must be a single ASCII character other than a quote or a line break",
            );
        }
        v.finish()
    }
}

/// A type inferred from sampled values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferredType {
    /// Every sampled value is empty.
    Unknown,
    Boolean,
    Integer,
    Decimal,
    Date,
    Timestamp,
    Uuid,
    Json,
    Text,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceColumn {
    pub index: usize,
    pub name: String,
    pub inferred_type: InferredType,
    /// Empty or missing values among the sampled rows.
    pub empty_count: u64,
    pub max_length: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub import_id: Uuid,
    pub format: FileFormat,
    pub delimiter: Option<char>,
    pub has_header: bool,
    pub columns: Vec<SourceColumn>,
    pub rows: Vec<Vec<Option<String>>>,
    pub total_rows: u64,
    pub sampled_rows: u64,
    /// Rows that cannot be read (wrong field count, invalid JSON, ...).
    pub parse_errors: Vec<RowError>,
    pub parse_error_count: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ColumnMapping {
    /// Index of the source column.
    pub source: usize,
    /// Target column name.
    pub target: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckImportRequest {
    pub schema: String,
    pub table: String,
    /// When absent, columns are matched by name.
    #[serde(default)]
    pub mapping: Option<Vec<ColumnMapping>>,
}

impl Validate for CheckImportRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_identifier(&mut v, "schema", &self.schema);
        validate_identifier(&mut v, "table", &self.table);
        if let Some(m) = &self.mapping {
            validate_mapping(&mut v, m);
        }
        v.finish()
    }
}

fn validate_mapping(v: &mut Validator, mapping: &[ColumnMapping]) {
    v.check(!mapping.is_empty(), "mapping", "map at least one column");
    v.check(mapping.len() <= 1600, "mapping", "too many columns");
    let mut seen = HashSet::new();
    for m in mapping {
        validate_identifier(v, "mapping.target", &m.target);
        v.check(
            seen.insert(m.target.as_str()),
            "mapping",
            &format!("\"{}\" is mapped more than once", m.target),
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetColumn {
    pub name: String,
    pub data_type: String,
    pub kind: &'static str,
    pub nullable: bool,
    pub has_default: bool,
    pub identity: Option<&'static str>,
    pub generated: bool,
    pub enum_values: Vec<String>,
    /// The column must be mapped (NOT NULL without a default).
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct MappingIssue {
    pub severity: Severity,
    pub source: Option<usize>,
    pub target: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CheckImportResponse {
    pub columns: Vec<TargetColumn>,
    pub mapping: Vec<ColumnMapping>,
    pub issues: Vec<MappingIssue>,
    /// Column sets usable as a conflict target (primary key and unique indexes).
    pub unique_keys: Vec<Vec<String>>,
    pub can_insert: bool,
    pub can_update: bool,
    pub connection_read_only: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateMode {
    /// Duplicate keys are row errors.
    #[default]
    Error,
    /// `ON CONFLICT DO NOTHING`.
    Skip,
    /// `ON CONFLICT (key) DO UPDATE`.
    Update,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorMode {
    /// Any invalid row rolls back the whole import.
    #[default]
    AllOrNothing,
    /// Invalid rows are reported and left out.
    SkipInvalid,
}

#[derive(Debug, Deserialize)]
pub struct RunImportRequest {
    pub schema: String,
    pub table: String,
    pub mapping: Vec<ColumnMapping>,
    #[serde(default)]
    pub duplicate_mode: DuplicateMode,
    /// Conflict target for `update` (and optionally `skip`).
    #[serde(default)]
    pub conflict_columns: Option<Vec<String>>,
    #[serde(default)]
    pub error_mode: ErrorMode,
    /// Empty fields become NULL.
    #[serde(default = "default_true")]
    pub empty_as_null: bool,
    #[serde(default)]
    pub confirm: Option<String>,
}

impl Validate for RunImportRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_identifier(&mut v, "schema", &self.schema);
        validate_identifier(&mut v, "table", &self.table);
        validate_mapping(&mut v, &self.mapping);
        if let Some(cols) = &self.conflict_columns {
            v.check(!cols.is_empty(), "conflict_columns", "must not be empty");
            for c in cols {
                validate_identifier(&mut v, "conflict_columns", c);
            }
        }
        v.check(
            self.duplicate_mode != DuplicateMode::Update || self.conflict_columns.is_some(),
            "conflict_columns",
            "choose the unique key that identifies existing rows",
        );
        if let Some(c) = &self.confirm {
            v.check(c.len() <= MAX_CONFIRM_CHARS, "confirm", "is too long");
        }
        v.finish()
    }
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportSource {
    Table(TableSource),
    /// The full result of one read-only statement.
    Query {
        sql: String,
        /// File name and `INSERT` target; defaults to `query_result`.
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
pub struct ExportsRequest {
    pub source: ExportSource,
    pub format: ExportFormat,
}

impl Validate for ExportsRequest {
    fn validate(&self) -> Result<(), AppError> {
        match &self.source {
            ExportSource::Table(t) => t.validate(),
            ExportSource::Query { sql, name } => {
                let mut v = Validator::new();
                v.check(!sql.trim().is_empty(), "source.sql", "is required");
                v.check(sql.len() <= MAX_SQL_BYTES, "source.sql", "is too long");
                if let Some(n) = name {
                    validate_identifier(&mut v, "source.name", n);
                }
                v.finish()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

/// How the values of one column are produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Generator {
    /// Leave the column out so its default (or identity/serial) applies.
    Default,
    Null,
    Constant {
        value: String,
    },
    FirstName,
    LastName,
    FullName,
    Email,
    Username,
    Phone,
    Company,
    ProductName,
    Word,
    Sentence,
    Paragraph,
    City,
    Country,
    CountryCode,
    StreetAddress,
    PostalCode,
    Url,
    Uuid,
    Boolean {
        #[serde(default = "half")]
        true_ratio: f64,
    },
    Integer {
        min: i64,
        max: i64,
    },
    /// Consecutive integers (unique).
    Sequence {
        #[serde(default)]
        start: Option<i64>,
    },
    Decimal {
        min: f64,
        max: f64,
        #[serde(default = "two")]
        scale: u32,
    },
    Date {
        from: String,
        to: String,
    },
    Timestamp {
        from: String,
        to: String,
    },
    /// One of a list of values (enum labels, CHECK lists, statuses).
    Values {
        values: Vec<String>,
    },
    Json,
    /// A key of the referenced table: generated parents or existing rows.
    Reference,
}

fn half() -> f64 {
    0.5
}
fn two() -> u32 {
    2
}

impl Generator {
    pub fn validate(&self, v: &mut Validator, field: &str) {
        match self {
            Generator::Integer { min, max } => {
                v.check(min <= max, field, "min must not exceed max");
            }
            Generator::Decimal { min, max, scale } => {
                v.check(
                    min.is_finite() && max.is_finite() && min <= max,
                    field,
                    "min must not exceed max",
                );
                v.check(*scale <= 10, field, "scale must be at most 10");
            }
            Generator::Boolean { true_ratio } => {
                v.check(
                    (0.0..=1.0).contains(true_ratio),
                    field,
                    "true_ratio must be between 0 and 1",
                );
            }
            Generator::Values { values } => {
                v.check(
                    !values.is_empty() && values.len() <= 1000,
                    field,
                    "list between 1 and 1000 values",
                );
                v.check(
                    values.iter().all(|s| s.len() <= 1000),
                    field,
                    "values are too long",
                );
            }
            Generator::Constant { value } => {
                v.check(value.len() <= 10_000, field, "value is too long");
            }
            Generator::Date { from, to } | Generator::Timestamp { from, to } => {
                v.check(
                    from.len() <= 40 && to.len() <= 40,
                    field,
                    "dates are too long",
                );
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MockTable {
    pub schema: String,
    pub table: String,
    pub rows: u64,
    /// Generator overrides by column name; other columns use the defaults.
    #[serde(default)]
    pub columns: BTreeMap<String, Generator>,
}

#[derive(Debug, Deserialize)]
pub struct MockRequest {
    pub tables: Vec<MockTable>,
    /// Same seed, same values.
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub confirm: Option<String>,
}

impl Validate for MockRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.check(
            !self.tables.is_empty() && self.tables.len() <= MAX_MOCK_TABLES,
            "tables",
            &format!("choose between 1 and {MAX_MOCK_TABLES} tables"),
        );
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for (i, t) in self.tables.iter().enumerate() {
            validate_identifier(&mut v, &format!("tables[{i}].schema"), &t.schema);
            validate_identifier(&mut v, &format!("tables[{i}].table"), &t.table);
            v.check(
                seen.insert((t.schema.as_str(), t.table.as_str())),
                "tables",
                &format!("{}.{} is listed twice", t.schema, t.table),
            );
            v.check(
                t.rows <= MAX_MOCK_ROWS_PER_TABLE,
                &format!("tables[{i}].rows"),
                &format!("at most {MAX_MOCK_ROWS_PER_TABLE} rows per table"),
            );
            total = total.saturating_add(t.rows);
            for (name, generator) in &t.columns {
                let field = format!("tables[{i}].columns.{name}");
                validate_identifier(&mut v, &field, name);
                generator.validate(&mut v, &field);
            }
        }
        v.check(
            self.seed.map_or(true, |s| s < (1 << 53)),
            "seed",
            "must be below 2^53",
        );
        v.check(
            total <= MAX_MOCK_ROWS_PER_RUN,
            "tables",
            &format!("at most {MAX_MOCK_ROWS_PER_RUN} rows per run"),
        );
        if let Some(c) = &self.confirm {
            v.check(c.len() <= MAX_CONFIRM_CHARS, "confirm", "is too long");
        }
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MockColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub generator: Generator,
    /// Referenced table for `reference` columns.
    pub references: Option<String>,
    /// Why this generator was chosen or what to watch out for.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MockTablePlan {
    pub schema: String,
    pub table: String,
    pub rows: u64,
    pub columns: Vec<MockColumn>,
    /// Sample rows over `columns` (NULL for columns left to their default).
    pub sample: Vec<Vec<Option<String>>>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MockPreview {
    pub seed: u64,
    /// Tables in the order they are filled.
    pub tables: Vec<MockTablePlan>,
    pub total_rows: u64,
    /// Parents with no rows that must be added to the run.
    pub missing_parents: Vec<String>,
    pub production_blocked: bool,
}

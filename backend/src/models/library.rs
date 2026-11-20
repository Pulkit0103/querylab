//! Query history, saved queries, collections and usage statistics.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::query::MAX_SQL_BYTES,
    utils::validation::{double_option, Validate, Validator},
};

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryStatus {
    Success,
    Error,
    Cancelled,
    Timeout,
}

impl HistoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HistoryStatus::Success => "success",
            HistoryStatus::Error => "error",
            HistoryStatus::Cancelled => "cancelled",
            HistoryStatus::Timeout => "timeout",
        }
    }
}

/// What produced a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    /// A statement run from the SQL editor.
    Query,
    /// A Data Studio change set.
    DataApply,
    Migration,
    Import,
}

impl HistoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HistoryKind::Query => "query",
            HistoryKind::DataApply => "data_apply",
            HistoryKind::Migration => "migration",
            HistoryKind::Import => "import",
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub sql_content: String,
    pub execution_time_ms: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub row_count: Option<i64>,
    pub executed_at: DateTime<Utc>,
    pub pinned: bool,
    pub kind: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryQuery {
    /// Page size (1-200, default 50).
    pub limit: Option<i64>,
    /// Cursor: return entries older than this history id.
    pub before: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub status: Option<HistoryStatus>,
    pub kind: Option<HistoryKind>,
    /// Case-insensitive substring match on the SQL text.
    pub q: Option<String>,
    /// Duration range in milliseconds (inclusive).
    pub min_ms: Option<i64>,
    pub max_ms: Option<i64>,
    /// Execution time range (inclusive start, exclusive end).
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub pinned: Option<bool>,
}

impl Validate for HistoryQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.check(
            [self.min_ms, self.max_ms].iter().flatten().all(|n| *n >= 0),
            "min_ms",
            "durations must not be negative",
        );
        if let (Some(min), Some(max)) = (self.min_ms, self.max_ms) {
            v.check(min <= max, "max_ms", "must not be less than min_ms");
        }
        if let (Some(from), Some(to)) = (self.from, self.to) {
            v.check(from < to, "to", "must be after from");
        }
        if let Some(q) = &self.q {
            v.length("q", q, 0, 1000);
        }
        v.finish()
    }
}

#[derive(Debug, Deserialize)]
pub struct PinRequest {
    pub pinned: bool,
}

impl Validate for PinRequest {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryPage {
    pub items: Vec<HistoryEntry>,
    /// Pass as `before` to fetch the next page; null on the last page.
    pub next_cursor: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

/// Folders nest at most this deep (a top-level folder has depth 1).
pub const MAX_FOLDER_DEPTH: i32 = 3;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    /// Parent folder; null for top-level folders.
    pub parent_id: Option<Uuid>,
    /// Queries directly in this folder.
    pub query_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn validate_folder_name(v: &mut Validator, name: &str) {
    v.length("name", name.trim(), 1, 100)
        .no_control_chars("name", name);
}

#[derive(Debug, Deserialize)]
pub struct CollectionRequest {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}

impl Validate for CollectionRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_folder_name(&mut v, &self.name);
        v.finish()
    }
}

/// Rename and/or move a folder. `parent_id: null` moves it to the top level;
/// an absent `parent_id` leaves it where it is.
#[derive(Debug, Deserialize, Default)]
pub struct UpdateCollectionRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id: Option<Option<Uuid>>,
}

impl Validate for UpdateCollectionRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(name) = &self.name {
            validate_folder_name(&mut v, name);
        }
        v.check(
            self.name.is_some() || self.parent_id.is_some(),
            "name",
            "provide a name or a parent_id",
        );
        v.finish()
    }
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

pub const MAX_TAGS: usize = 20;
pub const MAX_TAG_CHARS: usize = 40;

/// Lower-cases, trims and de-duplicates tags, keeping their first-seen order.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

fn validate_tags(v: &mut Validator, tags: &[String]) {
    let normalized = normalize_tags(tags);
    v.check(
        normalized.len() <= MAX_TAGS,
        "tags",
        &format!("at most {MAX_TAGS} tags"),
    )
    .check(
        normalized
            .iter()
            .all(|t| t.chars().count() <= MAX_TAG_CHARS),
        "tags",
        &format!("each tag must be at most {MAX_TAG_CHARS} characters"),
    )
    .check(
        normalized
            .iter()
            .all(|t| !t.contains(',') && !t.chars().any(char::is_control)),
        "tags",
        "tags must not contain commas or control characters",
    );
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

// ---------------------------------------------------------------------------
// Saved queries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SavedQuery {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub sql_content: String,
    pub connection_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub pinned: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSavedQuery {
    pub name: String,
    pub description: Option<String>,
    pub sql_content: String,
    pub connection_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Partial update. For the nullable references, an explicit `null` clears
/// the value while an absent field leaves it unchanged.
#[derive(Debug, Deserialize, Default)]
pub struct UpdateSavedQuery {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
    pub sql_content: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub connection_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub collection_id: Option<Option<Uuid>>,
    /// Replaces the tag list when present.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SavedQueryFilter {
    pub q: Option<String>,
    pub collection_id: Option<Uuid>,
    /// Comma-separated tags; a query must carry all of them.
    pub tags: Option<String>,
    pub pinned: Option<bool>,
}

impl SavedQueryFilter {
    pub fn tag_list(&self) -> Vec<String> {
        self.tags
            .as_deref()
            .map(|t| normalize_tags(&t.split(',').map(str::to_string).collect::<Vec<_>>()))
            .unwrap_or_default()
    }
}

fn validate_saved(
    v: &mut Validator,
    name: Option<&str>,
    description: Option<&str>,
    sql: Option<&str>,
) {
    if let Some(name) = name {
        v.length("name", name.trim(), 1, 200)
            .no_control_chars("name", name);
    }
    if let Some(d) = description {
        v.length("description", d, 0, 2000);
    }
    if let Some(sql) = sql {
        v.check(!sql.trim().is_empty(), "sql_content", "is required")
            .check(
                sql.len() <= MAX_SQL_BYTES,
                "sql_content",
                "must be at most 1 MiB",
            )
            .check(
                !sql.contains('\0'),
                "sql_content",
                "must not contain NUL bytes",
            );
    }
}

impl Validate for CreateSavedQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_saved(
            &mut v,
            Some(&self.name),
            self.description.as_deref(),
            Some(&self.sql_content),
        );
        validate_tags(&mut v, &self.tags);
        v.finish()
    }
}

impl Validate for UpdateSavedQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_saved(
            &mut v,
            self.name.as_deref(),
            self.description.as_ref().and_then(|d| d.as_deref()),
            self.sql_content.as_deref(),
        );
        if let Some(tags) = &self.tags {
            validate_tags(&mut v, tags);
        }
        v.finish()
    }
}

// ---------------------------------------------------------------------------
// Statistics (dashboard)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StatsTotals {
    pub executions: i64,
    pub successful: i64,
    pub failed: i64,
    pub avg_execution_ms: Option<f64>,
    pub p95_execution_ms: Option<f64>,
    pub saved_queries: i64,
    pub connections: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DailyStats {
    pub day: NaiveDate,
    pub successful: i64,
    pub failed: i64,
    pub avg_execution_ms: Option<f64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ConnectionUsage {
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub executions: i64,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub days: i32,
    pub totals: StatsTotals,
    pub daily: Vec<DailyStats>,
    pub by_connection: Vec<ConnectionUsage>,
    pub slowest: Vec<HistoryEntry>,
}

#[derive(Debug, Deserialize, Default)]
pub struct StatsQuery {
    pub days: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_distinguishes_null_from_absent() {
        let absent: UpdateSavedQuery = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
        assert!(absent.collection_id.is_none());
        let cleared: UpdateSavedQuery = serde_json::from_str(r#"{"collection_id":null}"#).unwrap();
        assert_eq!(cleared.collection_id, Some(None));
        let set: UpdateSavedQuery =
            serde_json::from_str(r#"{"collection_id":"00000000-0000-0000-0000-000000000001"}"#)
                .unwrap();
        assert!(matches!(set.collection_id, Some(Some(_))));
    }

    #[test]
    fn tags_are_normalized_and_limited() {
        let tags: Vec<String> = ["  Reports ", "reports", "", "KPI"]
            .map(String::from)
            .to_vec();
        assert_eq!(normalize_tags(&tags), ["reports", "kpi"]);

        let create = |tags: Vec<String>| CreateSavedQuery {
            name: "q".into(),
            description: None,
            sql_content: "select 1".into(),
            connection_id: None,
            collection_id: None,
            tags,
        };
        assert!(create((0..20).map(|i| format!("t{i}")).collect())
            .validate()
            .is_ok());
        assert!(create((0..21).map(|i| format!("t{i}")).collect())
            .validate()
            .is_err());
        assert!(create(vec!["x".repeat(41)]).validate().is_err());
        assert!(create(vec!["a,b".into()]).validate().is_err());
        // Duplicates collapse before the count is checked.
        assert!(create(vec!["Same".into(); 30]).validate().is_ok());

        let filter = SavedQueryFilter {
            tags: Some("Ops, ops ,daily,".into()),
            ..Default::default()
        };
        assert_eq!(filter.tag_list(), ["ops", "daily"]);
    }

    #[test]
    fn history_filters_are_checked() {
        let q = |s: &str| -> HistoryQuery { serde_urlencoded_like(s) };
        assert!(q("min_ms=10&max_ms=5").validate().is_err());
        assert!(q("min_ms=-1").validate().is_err());
        assert!(q("from=2026-01-02T00:00:00Z&to=2026-01-01T00:00:00Z")
            .validate()
            .is_err());
        assert!(q("min_ms=5&max_ms=10&from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z&pinned=true&kind=data_apply")
            .validate()
            .is_ok());
    }

    /// Parses a query string the way axum's `Query` extractor does.
    fn serde_urlencoded_like(s: &str) -> HistoryQuery {
        let uri: axum::http::Uri = format!("/?{s}").parse().unwrap();
        axum::extract::Query::<HistoryQuery>::try_from_uri(&uri)
            .unwrap()
            .0
    }

    #[test]
    fn folder_updates_need_a_change() {
        let empty: UpdateCollectionRequest = serde_json::from_str("{}").unwrap();
        assert!(empty.validate().is_err());
        let to_root: UpdateCollectionRequest =
            serde_json::from_str(r#"{"parent_id":null}"#).unwrap();
        assert_eq!(to_root.parent_id, Some(None));
        assert!(to_root.validate().is_ok());
    }

    #[test]
    fn saved_query_validation() {
        let bad: CreateSavedQuery =
            serde_json::from_str(r#"{"name":"","sql_content":"  "}"#).unwrap();
        assert!(bad.validate().is_err());
        let ok: CreateSavedQuery =
            serde_json::from_str(r#"{"name":"Top customers","sql_content":"select 1"}"#).unwrap();
        assert!(ok.validate().is_ok());
    }
}

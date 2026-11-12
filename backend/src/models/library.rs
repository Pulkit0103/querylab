//! Query history, saved queries, collections and usage statistics.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::query::MAX_SQL_BYTES,
    utils::validation::{Validate, Validator},
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
}

#[derive(Debug, Deserialize, Default)]
pub struct HistoryQuery {
    /// Page size (1-200, default 50).
    pub limit: Option<i64>,
    /// Cursor: return entries older than this history id.
    pub before: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub status: Option<HistoryStatus>,
    /// Case-insensitive substring match on the SQL text.
    pub q: Option<String>,
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

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub query_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CollectionRequest {
    pub name: String,
}

impl Validate for CollectionRequest {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .length("name", self.name.trim(), 1, 100)
            .no_control_chars("name", &self.name)
            .finish()
    }
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
}

#[derive(Debug, Deserialize)]
pub struct CreateSavedQuery {
    pub name: String,
    pub description: Option<String>,
    pub sql_content: String,
    pub connection_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
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
}

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize, Default)]
pub struct SavedQueryFilter {
    pub q: Option<String>,
    pub collection_id: Option<Uuid>,
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
    fn saved_query_validation() {
        let bad: CreateSavedQuery =
            serde_json::from_str(r#"{"name":"","sql_content":"  "}"#).unwrap();
        assert!(bad.validate().is_err());
        let ok: CreateSavedQuery =
            serde_json::from_str(r#"{"name":"Top customers","sql_content":"select 1"}"#).unwrap();
        assert!(ok.validate().is_ok());
    }
}

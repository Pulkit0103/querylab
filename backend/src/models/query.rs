use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    errors::AppError,
    utils::validation::{Validate, Validator},
};

/// SQL text is capped at 1 MiB (matches the history/saved-query limits).
pub const MAX_SQL_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub connection_id: Uuid,
    pub query: String,
    /// Requested row limit; clamped to the server maximum.
    pub max_rows: Option<u32>,
    /// Requested timeout; clamped to the server maximum.
    pub timeout_secs: Option<u32>,
    /// Client-chosen id used to cancel the execution while it runs.
    pub execution_id: Option<Uuid>,
}

fn validate_sql(v: &mut Validator, sql: &str) {
    v.check(!sql.trim().is_empty(), "query", "is required")
        .check(sql.len() <= MAX_SQL_BYTES, "query", "must be at most 1 MiB")
        .check(!sql.contains('\0'), "query", "must not contain NUL bytes");
}

impl Validate for ExecuteRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_sql(&mut v, &self.query);
        if let Some(n) = self.max_rows {
            v.check(n >= 1, "max_rows", "must be at least 1");
        }
        if let Some(t) = self.timeout_secs {
            v.check(t >= 1, "timeout_secs", "must be at least 1");
        }
        v.finish()
    }
}

#[derive(Debug, Deserialize)]
pub struct ExplainRequest {
    pub connection_id: Uuid,
    pub query: String,
    /// Run `EXPLAIN ANALYZE` (executes the statement inside a read-only,
    /// rolled-back transaction).
    #[serde(default)]
    pub analyze: bool,
    /// Explicit acknowledgement that ANALYZE executes the statement.
    #[serde(default)]
    pub confirm_analyze: bool,
    pub execution_id: Option<Uuid>,
    pub timeout_secs: Option<u32>,
}

impl Validate for ExplainRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_sql(&mut v, &self.query);
        v.check(
            !self.analyze || self.confirm_analyze,
            "confirm_analyze",
            "EXPLAIN ANALYZE executes the statement; confirm to proceed",
        );
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub status: &'static str,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    /// Rows returned in this response.
    pub row_count: usize,
    /// Rows affected by INSERT/UPDATE/DELETE/MERGE.
    pub rows_affected: Option<u64>,
    /// True when more rows existed than were returned.
    pub truncated: bool,
    pub execution_time_ms: u64,
    pub command: Option<String>,
    pub execution_id: Uuid,
    pub history_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ExplainResult {
    pub plan: Value,
    pub planning_time_ms: Option<f64>,
    pub execution_time_ms: Option<f64>,
    pub analyzed: bool,
    pub raw: Value,
}

#[derive(Debug, Serialize)]
pub struct CancelResponse {
    pub cancelled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        let ok: ExecuteRequest = serde_json::from_value(serde_json::json!({
            "connection_id": Uuid::nil(), "query": "select 1"
        }))
        .unwrap();
        assert!(ok.validate().is_ok());

        let empty: ExecuteRequest = serde_json::from_value(serde_json::json!({
            "connection_id": Uuid::nil(), "query": "   ", "max_rows": 0
        }))
        .unwrap();
        assert!(empty.validate().is_err());

        let unconfirmed: ExplainRequest = serde_json::from_value(serde_json::json!({
            "connection_id": Uuid::nil(), "query": "select 1", "analyze": true
        }))
        .unwrap();
        assert!(unconfirmed.validate().is_err());
    }
}

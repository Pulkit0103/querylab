//! Query performance history (grouped by fingerprint) and saved plans.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::{library::HistoryEntry, query::MAX_SQL_BYTES},
    utils::validation::{Validate, Validator},
};

/// Fingerprints are 64-bit; JSON numbers lose precision beyond 2^53, so
/// they travel as strings.
pub fn fingerprint_as_string<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

pub const DEFAULT_PERFORMANCE_DAYS: i32 = 14;
pub const MAX_PERFORMANCE_DAYS: i32 = 90;
/// Fingerprints listed per request (by total time).
pub const MAX_FINGERPRINTS: i64 = 100;
/// Saved plans kept per user.
pub const MAX_SAVED_PLANS: i64 = 500;
/// Serialized size limit of a saved plan (matches the table constraint).
pub const MAX_PLAN_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize, Default)]
pub struct PerformanceQuery {
    pub connection_id: Option<Uuid>,
    pub days: Option<i32>,
}

impl PerformanceQuery {
    pub fn days(&self) -> i32 {
        self.days.unwrap_or(DEFAULT_PERFORMANCE_DAYS)
    }
}

impl Validate for PerformanceQuery {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .check(
                (1..=MAX_PERFORMANCE_DAYS).contains(&self.days()),
                "days",
                &format!("must be between 1 and {MAX_PERFORMANCE_DAYS}"),
            )
            .finish()
    }
}

/// Timing of successful executions of one statement shape.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FingerprintStats {
    #[serde(serialize_with = "fingerprint_as_string")]
    pub fingerprint: i64,
    /// The most recent statement with this fingerprint (first 2000 chars).
    pub sample_sql: String,
    /// The statement with literals replaced by `?`.
    #[sqlx(skip)]
    pub normalized_sql: String,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub executions: i64,
    pub errors: i64,
    pub avg_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub max_ms: Option<i64>,
    pub total_ms: Option<i64>,
    pub last_executed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TimelinePoint {
    /// UTC day.
    pub day: NaiveDate,
    pub executions: i64,
    pub errors: i64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub max_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct FingerprintTimeline {
    pub stats: FingerprintStats,
    /// One point per UTC day in the range, including days without runs.
    pub points: Vec<TimelinePoint>,
    /// Latest executions with this fingerprint.
    pub recent: Vec<HistoryEntry>,
}

// ---------------------------------------------------------------------------
// Saved plans
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreatePlanRequest {
    pub name: String,
    #[serde(default)]
    pub connection_id: Option<Uuid>,
    pub sql: String,
    /// The EXPLAIN result as returned by `/api/query/explain`.
    pub plan: Value,
}

impl CreatePlanRequest {
    pub fn analyzed(&self) -> bool {
        self.plan
            .get("analyzed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

impl Validate for CreatePlanRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.length("name", self.name.trim(), 1, 200)
            .no_control_chars("name", &self.name)
            .check(
                !self.sql.trim().is_empty() && self.sql.len() <= MAX_SQL_BYTES,
                "sql",
                "must be between 1 byte and 1 MiB",
            )
            .check(
                !self.sql.contains('\0'),
                "sql",
                "must not contain NUL bytes",
            )
            .check(
                self.plan.get("plan").is_some_and(Value::is_object),
                "plan",
                "must be an EXPLAIN result with a `plan` object",
            );
        let size = serde_json::to_vec(&self.plan)
            .map(|b| b.len())
            .unwrap_or(usize::MAX);
        v.check(size <= MAX_PLAN_BYTES, "plan", "must be at most 2 MiB");
        v.finish()
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SavedPlanSummary {
    pub id: Uuid,
    pub name: String,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    /// First 300 characters of the statement.
    pub sql_preview: String,
    pub analyzed: bool,
    pub execution_time_ms: Option<f64>,
    pub total_cost: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SavedPlan {
    pub id: Uuid,
    pub name: String,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub sql_content: String,
    pub plan: sqlx::types::Json<Value>,
    pub analyzed: bool,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn days_are_bounded() {
        let q = |days| PerformanceQuery {
            connection_id: None,
            days,
        };
        assert!(q(None).validate().is_ok());
        assert_eq!(q(None).days(), DEFAULT_PERFORMANCE_DAYS);
        assert!(q(Some(0)).validate().is_err());
        assert!(q(Some(MAX_PERFORMANCE_DAYS + 1)).validate().is_err());
    }

    #[test]
    fn plans_need_a_plan_object() {
        let req = |plan: Value| CreatePlanRequest {
            name: "Before index".into(),
            connection_id: None,
            sql: "select 1".into(),
            plan,
        };
        let ok = req(json!({ "plan": { "Node Type": "Result" }, "analyzed": true }));
        assert!(ok.validate().is_ok());
        assert!(ok.analyzed());
        assert!(req(json!({ "plan": [] })).validate().is_err());
        assert!(req(json!([1])).validate().is_err());
        let mut blank = req(json!({ "plan": {} }));
        blank.name = "  ".into();
        assert!(blank.validate().is_err());
        assert!(!req(json!({ "plan": {} })).analyzed());
    }

    #[test]
    fn fingerprints_serialize_as_strings() {
        let v = serde_json::to_value(TimelineFingerprint(i64::MIN)).unwrap();
        assert_eq!(v, json!(i64::MIN.to_string()));
    }

    #[derive(Serialize)]
    struct TimelineFingerprint(#[serde(serialize_with = "fingerprint_as_string")] i64);
}

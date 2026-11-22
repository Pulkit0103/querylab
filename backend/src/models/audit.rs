//! Audit log records and queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    errors::AppError,
    utils::validation::{Validate, Validator},
};

pub const MAX_AUDIT_PAGE: i64 = 500;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditEventRecord {
    pub id: i64,
    pub workspace_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub actor_email: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub detail: Option<Value>,
    pub ip: Option<String>,
    pub request_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuditQuery {
    /// Exact action, or a prefix ending in `.` (e.g. `auth.`).
    pub action: Option<String>,
    pub actor: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Keyset cursor: events with a smaller id.
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

impl Validate for AuditQuery {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        if let Some(a) = &self.action {
            v.length("action", a, 1, 100);
        }
        if let Some(l) = self.limit {
            v.check(
                (1..=MAX_AUDIT_PAGE).contains(&l),
                "limit",
                "must be between 1 and 500",
            );
        }
        v.finish()
    }
}

#[derive(Debug, Serialize)]
pub struct AuditPage {
    pub events: Vec<AuditEventRecord>,
    pub next_before: Option<i64>,
}

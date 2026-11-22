//! Reading the audit log.

use uuid::Uuid;

use crate::{
    database::audit as repo,
    errors::AppResult,
    models::audit::{AuditEventRecord, AuditPage, AuditQuery, MAX_AUDIT_PAGE},
    services::{auth::AuthUser, sql_builder::csv_line},
    state::AppState,
    utils::validation::Validate,
};

/// Rows in one CSV export.
pub const MAX_EXPORT_EVENTS: i64 = 50_000;

pub struct AuditLogService<'a> {
    state: &'a AppState,
}

impl<'a> AuditLogService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Workspaces whose events the user may read (administrators only).
    async fn readable_workspaces(&self, _user_id: Uuid) -> AppResult<Vec<Uuid>> {
        Ok(Vec::new())
    }

    pub async fn page(&self, user: &AuthUser, q: &AuditQuery) -> AppResult<AuditPage> {
        q.validate()?;
        let limit = q.limit.unwrap_or(100).clamp(1, MAX_AUDIT_PAGE);
        let workspaces = self.readable_workspaces(user.user_id).await?;
        let events = repo::list(&self.state.db, user.user_id, &workspaces, q, limit + 1).await?;
        let more = events.len() as i64 > limit;
        let events: Vec<AuditEventRecord> = events.into_iter().take(limit as usize).collect();
        Ok(AuditPage {
            next_before: if more {
                events.last().map(|e| e.id)
            } else {
                None
            },
            events,
        })
    }

    pub async fn csv(&self, user: &AuthUser, q: &AuditQuery) -> AppResult<String> {
        q.validate()?;
        let workspaces = self.readable_workspaces(user.user_id).await?;
        let events = repo::list(
            &self.state.db,
            user.user_id,
            &workspaces,
            q,
            MAX_EXPORT_EVENTS,
        )
        .await?;
        let mut out = csv_line(
            [
                "time",
                "actor",
                "action",
                "target_type",
                "target_id",
                "connection",
                "ip",
                "request_id",
                "detail",
            ]
            .into_iter()
            .map(Some),
        );
        for e in events {
            let row = [
                Some(e.created_at.to_rfc3339()),
                e.actor_email.or(e.actor_id.map(|a| a.to_string())),
                Some(e.action),
                e.target_type,
                e.target_id.map(|t| t.to_string()),
                e.connection_name.or(e.connection_id.map(|c| c.to_string())),
                e.ip,
                e.request_id.map(|r| r.to_string()),
                e.detail.map(|d| d.to_string()),
            ];
            out.push_str(&csv_line(row.iter().map(|v| v.as_deref())));
        }
        Ok(out)
    }
}

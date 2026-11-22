//! The audit log: who did what, from where.
//!
//! * Events are append-only (a trigger rejects updates and early deletes).
//! * The client address and request id are captured per request by
//!   [`capture`] and read by [`record`], so services only describe the event.
//! * Executed SQL is truncated to 10 KB; bound parameters are never stored.
//! * Recording never fails the action it describes: a failed insert is
//!   retried once and then logged at error level.

use std::time::Duration;

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    database::audit::{self as repo, NewAuditEvent},
    middleware::{client_ip, observability::REQUEST_ID},
    state::AppState,
};

/// Executed SQL kept in an event.
pub const MAX_AUDIT_SQL_BYTES: usize = 10 * 1024;

#[derive(Debug, Clone, Default)]
pub struct RequestMeta {
    pub ip: Option<String>,
    pub request_id: Option<Uuid>,
}

tokio::task_local! {
    static REQUEST_META: RequestMeta;
}

/// Middleware: makes the request's client address and id available to
/// [`record`] for the duration of the request.
pub async fn capture(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip());
    let ip = client_ip::resolve(req.headers(), peer, state.config.trusted_proxy_hops);
    let meta = RequestMeta {
        ip: Some(ip.to_string()),
        request_id: req
            .headers()
            .get(&REQUEST_ID)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok()),
    };
    REQUEST_META.scope(meta, next.run(req)).await
}

/// The current request's metadata (empty outside a request).
pub fn current() -> RequestMeta {
    REQUEST_META.try_with(Clone::clone).unwrap_or_default()
}

/// Spawns background work (imports, mock data, exports) that keeps the
/// request's address and id for the events it records.
pub fn spawn_in_request<F>(fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(REQUEST_META.scope(current(), fut))
}

/// SQL as stored in an event: at most 10 KB, cut at a character boundary.
pub fn sql_excerpt(sql: &str) -> String {
    let mut end = sql.len().min(MAX_AUDIT_SQL_BYTES);
    while !sql.is_char_boundary(end) {
        end -= 1;
    }
    sql[..end].to_string()
}

/// One event to record.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub actor_id: Option<Uuid>,
    pub action: &'static str,
    pub target_type: Option<&'static str>,
    pub target_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub detail: Value,
}

impl AuditEvent {
    pub fn new(actor_id: Option<Uuid>, action: &'static str) -> Self {
        Self {
            actor_id,
            action,
            target_type: None,
            target_id: None,
            connection_id: None,
            workspace_id: None,
            detail: Value::Null,
        }
    }

    pub fn target(mut self, kind: &'static str, id: Uuid) -> Self {
        self.target_type = Some(kind);
        self.target_id = Some(id);
        self
    }

    pub fn connection(mut self, id: Uuid) -> Self {
        self.connection_id = Some(id);
        self
    }

    pub fn workspace(mut self, id: Option<Uuid>) -> Self {
        self.workspace_id = id;
        self
    }

    pub fn detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn sql(mut self, sql: &str) -> Self {
        let excerpt = sql_excerpt(sql);
        match &mut self.detail {
            Value::Object(map) => {
                map.insert("sql".into(), Value::String(excerpt));
            }
            _ => self.detail = json!({ "sql": excerpt }),
        }
        self
    }
}

/// Records an event with the current request's address and id.
pub async fn record(db: &PgPool, event: AuditEvent) {
    record_with(db, event, current()).await;
}

pub async fn record_with(db: &PgPool, event: AuditEvent, meta: RequestMeta) {
    let detail = (!event.detail.is_null()).then_some(&event.detail);
    let row = NewAuditEvent {
        workspace_id: event.workspace_id,
        actor_id: event.actor_id,
        action: event.action,
        target_type: event.target_type,
        target_id: event.target_id,
        connection_id: event.connection_id,
        detail,
        ip: meta.ip.as_deref(),
        request_id: meta.request_id,
    };
    if let Err(first) = repo::insert(db, &row).await {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Err(err) = repo::insert(db, &row).await {
            tracing::error!(action = event.action, error = %err, first_error = %first, "failed to record audit event");
        }
    }
}

/// Purges events past the retention period once a day.
pub fn spawn_retention(db: PgPool, retention_days: u32) {
    if retention_days == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(24 * 3600));
        loop {
            interval.tick().await;
            match repo::purge(&db, retention_days).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(removed = n, "old audit events purged"),
                Err(e) => tracing::warn!(error = %e, "audit purge failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_is_truncated_at_a_character_boundary() {
        let sql = "é".repeat(MAX_AUDIT_SQL_BYTES);
        let excerpt = sql_excerpt(&sql);
        assert!(excerpt.len() <= MAX_AUDIT_SQL_BYTES);
        assert!(excerpt.chars().all(|c| c == 'é'));
        let e = AuditEvent::new(None, "query.executed")
            .detail(json!({ "status": "success" }))
            .sql("select 1");
        assert_eq!(e.detail, json!({ "status": "success", "sql": "select 1" }));
    }

    #[tokio::test]
    async fn request_meta_is_scoped() {
        assert!(current().ip.is_none());
        let meta = RequestMeta {
            ip: Some("10.0.0.1".into()),
            request_id: None,
        };
        let seen = REQUEST_META.scope(meta, async { current() }).await;
        assert_eq!(seen.ip.as_deref(), Some("10.0.0.1"));
    }
}

//! Audit events (metadata database). The table is append-only: a trigger
//! rejects updates and early deletes.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::audit::{AuditEventRecord, AuditQuery};

pub struct NewAuditEvent<'a> {
    pub workspace_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub action: &'a str,
    pub target_type: Option<&'a str>,
    pub target_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub detail: Option<&'a serde_json::Value>,
    pub ip: Option<&'a str>,
    pub request_id: Option<Uuid>,
}

pub async fn insert(db: &PgPool, e: &NewAuditEvent<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_events (workspace_id, actor_id, action, target_type, target_id, \
                                   connection_id, detail, ip, request_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(e.workspace_id)
    .bind(e.actor_id)
    .bind(e.action)
    .bind(e.target_type)
    .bind(e.target_id)
    .bind(e.connection_id)
    .bind(e.detail)
    .bind(e.ip)
    .bind(e.request_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Events visible under `scope`: the actor's own events, plus events of the
/// given workspaces (for workspace administrators).
pub async fn list(
    db: &PgPool,
    actor_scope: Uuid,
    workspaces: &[Uuid],
    q: &AuditQuery,
    limit: i64,
) -> Result<Vec<AuditEventRecord>, sqlx::Error> {
    let (exact, prefix) = match q.action.as_deref() {
        Some(a) if a.ends_with('.') => (
            None,
            Some(format!("{}%", a.replace('%', "\\%").replace('_', "\\_"))),
        ),
        Some(a) => (Some(a.to_string()), None),
        None => (None, None),
    };
    sqlx::query_as(
        "SELECT e.id, e.workspace_id, e.actor_id, u.email AS actor_email, e.action, e.target_type, \
                e.target_id, e.connection_id, c.name AS connection_name, e.detail, e.ip, e.request_id, \
                e.created_at \
         FROM audit_events e \
         LEFT JOIN users u ON u.id = e.actor_id \
         LEFT JOIN database_connections c ON c.id = e.connection_id \
         WHERE (e.actor_id = $1 OR e.workspace_id = ANY($2)) \
           AND ($3::text IS NULL OR e.action = $3) \
           AND ($4::text IS NULL OR e.action LIKE $4) \
           AND ($5::uuid IS NULL OR e.actor_id = $5) \
           AND ($6::uuid IS NULL OR e.connection_id = $6) \
           AND ($7::uuid IS NULL OR e.workspace_id = $7) \
           AND ($8::timestamptz IS NULL OR e.created_at >= $8) \
           AND ($9::timestamptz IS NULL OR e.created_at < $9) \
           AND ($10::bigint IS NULL OR e.id < $10) \
         ORDER BY e.id DESC LIMIT $11",
    )
    .bind(actor_scope)
    .bind(workspaces)
    .bind(exact)
    .bind(prefix)
    .bind(q.actor)
    .bind(q.connection_id)
    .bind(q.workspace_id)
    .bind(q.from)
    .bind(q.to)
    .bind(q.before)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Deletes events older than the retention period (the guard trigger
/// allows exactly these rows).
pub async fn purge(db: &PgPool, retention_days: u32) -> Result<u64, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('querylab.audit_retention_days', $1, true)")
        .bind(retention_days.to_string())
        .execute(&mut *tx)
        .await?;
    let done = sqlx::query(
        "DELETE FROM audit_events WHERE created_at < now() - make_interval(days => $1)",
    )
    .bind(retention_days as i32)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(done.rows_affected())
}

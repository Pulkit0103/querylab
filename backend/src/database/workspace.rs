//! Repositories for the synced editor session and workspace settings.

use chrono::{DateTime, Utc};
use sqlx::{types::Json, PgPool};
use uuid::Uuid;

use crate::models::workspace::{WorkspaceSession, WorkspaceSettings};

pub async fn get_session(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<(WorkspaceSession, DateTime<Utc>)>, sqlx::Error> {
    let row: Option<(Json<WorkspaceSession>, DateTime<Utc>)> =
        sqlx::query_as("SELECT session, updated_at FROM workspace_sessions WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(Json(s), at)| (s, at)))
}

pub async fn put_session(
    db: &PgPool,
    user_id: Uuid,
    session: &WorkspaceSession,
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO workspace_sessions (user_id, session, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (user_id) DO UPDATE SET session = EXCLUDED.session, updated_at = now() \
         RETURNING updated_at",
    )
    .bind(user_id)
    .bind(Json(session))
    .fetch_one(db)
    .await
}

pub async fn get_settings(db: &PgPool, user_id: Uuid) -> Result<WorkspaceSettings, sqlx::Error> {
    sqlx::query_as("SELECT history_retention_days FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await
}

pub async fn put_settings(
    db: &PgPool,
    user_id: Uuid,
    settings: &WorkspaceSettings,
) -> Result<WorkspaceSettings, sqlx::Error> {
    sqlx::query_as(
        "UPDATE users SET history_retention_days = $2 WHERE id = $1 RETURNING history_retention_days",
    )
    .bind(user_id)
    .bind(settings.history_retention_days)
    .fetch_one(db)
    .await
}

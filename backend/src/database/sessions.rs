use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Session joined with its user, as needed by the auth extractor.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionWithUser {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub async fn insert(
    db: &PgPool,
    id: Uuid,
    user_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
    user_agent: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at, user_agent) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .execute(db)
    .await?;
    Ok(())
}

/// Finds a live (unexpired) session by token hash.
pub async fn find_active(
    db: &PgPool,
    token_hash: &[u8],
) -> Result<Option<SessionWithUser>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.id AS session_id, s.user_id, u.email, u.display_name, s.created_at, s.last_seen_at, s.expires_at \
         FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.token_hash = $1 AND s.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await
}

pub async fn extend(db: &PgPool, id: Uuid, expires_at: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE sessions SET last_seen_at = now(), expires_at = $2 WHERE id = $1")
        .bind(id)
        .bind(expires_at)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn delete_all_for_user(db: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected())
}

pub async fn delete_expired(db: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(db)
            .await?
            .rows_affected(),
    )
}

//! Account security records: TOTP, recovery codes, login challenges,
//! password resets and the session list. Every statement is scoped by
//! user id or by a token hash.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TotpState {
    pub secret: Option<Vec<u8>>,
    pub enabled_at: Option<DateTime<Utc>>,
    pub last_step: Option<i64>,
}

pub async fn totp_state(db: &PgPool, user_id: Uuid) -> Result<TotpState, sqlx::Error> {
    sqlx::query_as(
        "SELECT totp_secret_encrypted AS secret, totp_enabled_at AS enabled_at, \
                totp_last_step AS last_step FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

/// Stores a new secret while 2FA is not enabled yet.
pub async fn set_pending_totp(
    db: &PgPool,
    user_id: Uuid,
    secret: &[u8],
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE users SET totp_secret_encrypted = $2, totp_last_step = NULL \
         WHERE id = $1 AND totp_enabled_at IS NULL",
    )
    .bind(user_id)
    .bind(secret)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Records a used time step unless a later one was already used (replay
/// protection that holds under concurrent requests).
pub async fn use_totp_step(db: &PgPool, user_id: Uuid, step: i64) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE users SET totp_last_step = $2 \
         WHERE id = $1 AND (totp_last_step IS NULL OR totp_last_step < $2)",
    )
    .bind(user_id)
    .bind(step)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Enables 2FA and replaces the recovery codes.
pub async fn enable_totp(
    db: &PgPool,
    user_id: Uuid,
    code_hashes: &[Vec<u8>],
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE users SET totp_enabled_at = now() WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO totp_recovery_codes (user_id, code_hash) SELECT $1, unnest($2::bytea[])",
    )
    .bind(user_id)
    .bind(code_hashes)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

pub async fn disable_totp(db: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE users SET totp_secret_encrypted = NULL, totp_enabled_at = NULL, totp_last_step = NULL \
         WHERE id = $1",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Marks a recovery code used; false when unknown or already used.
pub async fn use_recovery_code(
    db: &PgPool,
    user_id: Uuid,
    code_hash: &[u8],
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE totp_recovery_codes SET used_at = now() \
         WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
    )
    .bind(user_id)
    .bind(code_hash)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn recovery_codes_left(db: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM totp_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

// ---------------------------------------------------------------------------
// Login challenges (second factor)
// ---------------------------------------------------------------------------

pub async fn insert_challenge(
    db: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
    user_agent: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO login_challenges (id, user_id, token_hash, expires_at, user_agent) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .execute(db)
    .await?;
    Ok(())
}

/// Counts an attempt on a live challenge and returns its user, or `None`
/// when the challenge is unknown, expired or out of attempts.
pub async fn attempt_challenge(
    db: &PgPool,
    token_hash: &[u8],
    max_attempts: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE login_challenges SET attempts = attempts + 1 \
         WHERE token_hash = $1 AND expires_at > now() AND attempts < $2 RETURNING user_id",
    )
    .bind(token_hash)
    .bind(max_attempts)
    .fetch_optional(db)
    .await
}

pub async fn delete_challenge(db: &PgPool, token_hash: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM login_challenges WHERE token_hash = $1")
        .bind(token_hash)
        .execute(db)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Password resets
// ---------------------------------------------------------------------------

pub async fn insert_reset(
    db: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    // Only the newest link works.
    sqlx::query(
        "UPDATE password_resets SET used_at = now() WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .execute(db)
    .await?;
    sqlx::query(
        "INSERT INTO password_resets (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Consumes a reset token: returns its user once, if it is live.
pub async fn consume_reset(db: &PgPool, token_hash: &[u8]) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE password_resets SET used_at = now() \
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now() RETURNING user_id",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await
}

/// Whether a reset token is live (without consuming it).
pub async fn reset_is_live(db: &PgPool, token_hash: &[u8]) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM password_resets \
                        WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now())",
    )
    .bind(token_hash)
    .fetch_one(db)
    .await
}

/// Reset requests for a user in the last hour (abuse limit).
pub async fn recent_resets(db: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM password_resets WHERE user_id = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SessionInfo {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    #[sqlx(default)]
    pub current: bool,
}

pub async fn list_sessions(db: &PgPool, user_id: Uuid) -> Result<Vec<SessionInfo>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, created_at, last_seen_at, expires_at, user_agent, ip, false AS current \
         FROM sessions WHERE user_id = $1 AND expires_at > now() ORDER BY last_seen_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn delete_session(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// Signs out everywhere else.
pub async fn delete_other_sessions(
    db: &PgPool,
    user_id: Uuid,
    keep: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND id <> $2")
            .bind(user_id)
            .bind(keep)
            .execute(db)
            .await?
            .rows_affected(),
    )
}

pub async fn delete_expired(db: &PgPool) -> Result<u64, sqlx::Error> {
    let a = sqlx::query("DELETE FROM login_challenges WHERE expires_at <= now()")
        .execute(db)
        .await?
        .rows_affected();
    let b =
        sqlx::query("DELETE FROM password_resets WHERE expires_at <= now() - interval '7 days'")
            .execute(db)
            .await?
            .rows_affected();
    Ok(a + b)
}

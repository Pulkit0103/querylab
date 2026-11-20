//! Repository for `database_connections` and `connection_groups`. Every query
//! is scoped by `user_id` so ownership is enforced in SQL, not only in
//! application code.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::connection::{ConnectionGroup, ConnectionRecord};

const COLUMNS: &str = "id, user_id, name, database_type, host, port, database_name, username, \
     ssl_mode, read_only, encrypted_credentials, created_at, updated_at, last_connected_at, \
     environment, group_id, color, ssl_root_cert, last_status, last_status_at, last_latency_ms, last_error";

pub struct NewConnection<'a> {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: &'a str,
    pub host: &'a str,
    pub port: u16,
    pub database_name: &'a str,
    pub username: &'a str,
    pub ssl_mode: &'a str,
    pub read_only: bool,
    pub encrypted_credentials: &'a [u8],
    pub environment: &'a str,
    pub group_id: Option<Uuid>,
    pub color: Option<&'a str>,
    pub ssl_root_cert: Option<&'a str>,
}

pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM database_connections WHERE user_id = $1 ORDER BY lower(name), created_at"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn find_for_user(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM database_connections WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn name_exists(db: &PgPool, user_id: Uuid, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM database_connections WHERE user_id = $1 AND name = $2)",
    )
    .bind(user_id)
    .bind(name)
    .fetch_one(db)
    .await
}

pub async fn insert(db: &PgPool, c: &NewConnection<'_>) -> Result<ConnectionRecord, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO database_connections \
            (id, user_id, name, database_type, host, port, database_name, username, ssl_mode, read_only, \
             encrypted_credentials, environment, group_id, color, ssl_root_cert) \
         VALUES ($1, $2, $3, 'postgresql', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
         RETURNING {COLUMNS}"
    ))
    .bind(c.id)
    .bind(c.user_id)
    .bind(c.name)
    .bind(c.host)
    .bind(i32::from(c.port))
    .bind(c.database_name)
    .bind(c.username)
    .bind(c.ssl_mode)
    .bind(c.read_only)
    .bind(c.encrypted_credentials)
    .bind(c.environment)
    .bind(c.group_id)
    .bind(c.color)
    .bind(c.ssl_root_cert)
    .fetch_one(db)
    .await
}

/// Replaces all mutable fields. Callers merge partial updates first. When
/// `reset_status` is set (the target or credentials changed) the last test
/// result is cleared because it no longer describes this connection.
pub async fn update(
    db: &PgPool,
    c: &NewConnection<'_>,
    reset_status: bool,
) -> Result<Option<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "UPDATE database_connections SET \
            name = $3, host = $4, port = $5, database_name = $6, username = $7, \
            ssl_mode = $8, read_only = $9, encrypted_credentials = $10, \
            environment = $11, group_id = $12, color = $13, ssl_root_cert = $14, \
            last_status = CASE WHEN $15 THEN NULL ELSE last_status END, \
            last_status_at = CASE WHEN $15 THEN NULL ELSE last_status_at END, \
            last_latency_ms = CASE WHEN $15 THEN NULL ELSE last_latency_ms END, \
            last_error = CASE WHEN $15 THEN NULL ELSE last_error END \
         WHERE id = $1 AND user_id = $2 \
         RETURNING {COLUMNS}"
    ))
    .bind(c.id)
    .bind(c.user_id)
    .bind(c.name)
    .bind(c.host)
    .bind(i32::from(c.port))
    .bind(c.database_name)
    .bind(c.username)
    .bind(c.ssl_mode)
    .bind(c.read_only)
    .bind(c.encrypted_credentials)
    .bind(c.environment)
    .bind(c.group_id)
    .bind(c.color)
    .bind(c.ssl_root_cert)
    .bind(reset_status)
    .fetch_optional(db)
    .await
}

pub async fn delete_for_user(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM database_connections WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Records the outcome of a connection test. A successful test also updates
/// `last_connected_at`.
pub async fn record_status(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    status: &str,
    latency_ms: Option<i32>,
    error: Option<&str>,
) -> Result<Option<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "UPDATE database_connections SET \
            last_status = $3, last_status_at = now(), last_latency_ms = $4, last_error = $5, \
            last_connected_at = CASE WHEN $3 = 'online' THEN now() ELSE last_connected_at END \
         WHERE id = $1 AND user_id = $2 \
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(status)
    .bind(latency_ms)
    .bind(error)
    .fetch_optional(db)
    .await
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

const GROUP_COLUMNS: &str = "g.id, g.name, g.position, g.created_at, g.updated_at, \
     (SELECT count(*) FROM database_connections c WHERE c.group_id = g.id) AS connection_count";

pub async fn list_groups(db: &PgPool, user_id: Uuid) -> Result<Vec<ConnectionGroup>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {GROUP_COLUMNS} FROM connection_groups g WHERE g.user_id = $1 \
         ORDER BY g.position, lower(g.name)"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn insert_group(
    db: &PgPool,
    user_id: Uuid,
    name: &str,
    position: i32,
) -> Result<ConnectionGroup, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH g AS (INSERT INTO connection_groups (id, user_id, name, position) \
                    VALUES ($1, $2, $3, $4) RETURNING *) \
         SELECT {GROUP_COLUMNS} FROM g"
    ))
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(name)
    .bind(position)
    .fetch_one(db)
    .await
}

pub async fn update_group(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    name: &str,
    position: Option<i32>,
) -> Result<Option<ConnectionGroup>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH g AS (UPDATE connection_groups SET name = $3, position = COALESCE($4, position) \
                    WHERE id = $1 AND user_id = $2 RETURNING *) \
         SELECT {GROUP_COLUMNS} FROM g"
    ))
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(position)
    .fetch_optional(db)
    .await
}

pub async fn delete_group(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM connection_groups WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn next_group_position(db: &PgPool, user_id: Uuid) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(max(position) + 1, 0) FROM connection_groups WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

//! Repository for `database_connections` and `connection_groups`.
//!
//! Reads that serve a user join `workspace_members` (and `connection_grants`)
//! so visibility is enforced in SQL; the service then applies the access
//! rules of `services::authz`. Writes address rows by id after that check.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::connection::{ConnectionGroup, ConnectionRecord};

const COLUMNS: &str = "c.id, c.user_id, c.workspace_id, c.name, c.database_type, c.host, c.port, \
     c.database_name, c.username, c.ssl_mode, c.read_only, c.encrypted_credentials, c.created_at, \
     c.updated_at, c.last_connected_at, c.environment, c.group_id, c.color, c.ssl_root_cert, \
     c.last_status, c.last_status_at, c.last_latency_ms, c.last_error";

pub struct NewConnection<'a> {
    pub id: Uuid,
    /// Creator; the credentials are bound to it.
    pub user_id: Uuid,
    pub workspace_id: Uuid,
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

/// A connection with the caller's role in its workspace and explicit grant.
#[derive(sqlx::FromRow)]
pub struct WithAccess {
    #[sqlx(flatten)]
    pub record: ConnectionRecord,
    pub member_role: String,
    pub grant_access: Option<String>,
}

const ACCESS_JOIN: &str = "FROM database_connections c \
     JOIN workspace_members m ON m.workspace_id = c.workspace_id AND m.user_id = $1 \
     LEFT JOIN connection_grants g ON g.connection_id = c.id AND g.user_id = $1";

/// Connections in the user's workspaces (optionally one workspace).
pub async fn list_with_access(
    db: &PgPool,
    user_id: Uuid,
    workspace_id: Option<Uuid>,
) -> Result<Vec<WithAccess>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS}, m.role AS member_role, g.access AS grant_access {ACCESS_JOIN} \
         WHERE ($2::uuid IS NULL OR c.workspace_id = $2) ORDER BY lower(c.name), c.created_at"
    ))
    .bind(user_id)
    .bind(workspace_id)
    .fetch_all(db)
    .await
}

pub async fn find_with_access(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<(ConnectionRecord, String, Option<String>)>, sqlx::Error> {
    let row: Option<WithAccess> = sqlx::query_as(&format!(
        "SELECT {COLUMNS}, m.role AS member_role, g.access AS grant_access {ACCESS_JOIN} WHERE c.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.record, r.member_role, r.grant_access)))
}

pub async fn name_exists(db: &PgPool, workspace_id: Uuid, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM database_connections WHERE workspace_id = $1 AND name = $2)",
    )
    .bind(workspace_id)
    .bind(name)
    .fetch_one(db)
    .await
}

pub async fn insert(db: &PgPool, c: &NewConnection<'_>) -> Result<ConnectionRecord, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH c AS (INSERT INTO database_connections \
            (id, user_id, workspace_id, name, database_type, host, port, database_name, username, ssl_mode, \
             read_only, encrypted_credentials, environment, group_id, color, ssl_root_cert) \
         VALUES ($1, $2, $3, $4, 'postgresql', $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
         RETURNING *) SELECT {COLUMNS} FROM c"
    ))
    .bind(c.id)
    .bind(c.user_id)
    .bind(c.workspace_id)
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

/// Replaces all mutable fields (the creator and workspace never change).
/// Callers merge partial updates first. When `reset_status` is set (the
/// target or credentials changed) the last test result is cleared because
/// it no longer describes this connection.
pub async fn update(
    db: &PgPool,
    c: &NewConnection<'_>,
    reset_status: bool,
) -> Result<Option<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH c AS (UPDATE database_connections SET \
            name = $2, host = $3, port = $4, database_name = $5, username = $6, \
            ssl_mode = $7, read_only = $8, encrypted_credentials = $9, \
            environment = $10, group_id = $11, color = $12, ssl_root_cert = $13, \
            last_status = CASE WHEN $14 THEN NULL ELSE last_status END, \
            last_status_at = CASE WHEN $14 THEN NULL ELSE last_status_at END, \
            last_latency_ms = CASE WHEN $14 THEN NULL ELSE last_latency_ms END, \
            last_error = CASE WHEN $14 THEN NULL ELSE last_error END \
         WHERE id = $1 RETURNING *) SELECT {COLUMNS} FROM c"
    ))
    .bind(c.id)
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

pub async fn delete(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM database_connections WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Records the outcome of a connection test. A successful test also updates
/// `last_connected_at`.
pub async fn record_status(
    db: &PgPool,
    id: Uuid,
    status: &str,
    latency_ms: Option<i32>,
    error: Option<&str>,
) -> Result<Option<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH c AS (UPDATE database_connections SET \
            last_status = $2, last_status_at = now(), last_latency_ms = $3, last_error = $4, \
            last_connected_at = CASE WHEN $2 = 'online' THEN now() ELSE last_connected_at END \
         WHERE id = $1 RETURNING *) SELECT {COLUMNS} FROM c"
    ))
    .bind(id)
    .bind(status)
    .bind(latency_ms)
    .bind(error)
    .fetch_optional(db)
    .await
}

// ---------------------------------------------------------------------------
// Groups (per workspace)
// ---------------------------------------------------------------------------

const GROUP_COLUMNS: &str = "g.id, g.name, g.position, g.created_at, g.updated_at, \
     (SELECT count(*) FROM database_connections c WHERE c.group_id = g.id) AS connection_count";

pub async fn list_groups(
    db: &PgPool,
    workspace_id: Uuid,
) -> Result<Vec<ConnectionGroup>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {GROUP_COLUMNS} FROM connection_groups g WHERE g.workspace_id = $1 \
         ORDER BY g.position, lower(g.name)"
    ))
    .bind(workspace_id)
    .fetch_all(db)
    .await
}

pub async fn group_workspace(db: &PgPool, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT workspace_id FROM connection_groups WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn insert_group(
    db: &PgPool,
    user_id: Uuid,
    workspace_id: Uuid,
    name: &str,
    position: i32,
) -> Result<ConnectionGroup, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH g AS (INSERT INTO connection_groups (id, user_id, workspace_id, name, position) \
                    VALUES ($1, $2, $3, $4, $5) RETURNING *) \
         SELECT {GROUP_COLUMNS} FROM g"
    ))
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(workspace_id)
    .bind(name)
    .bind(position)
    .fetch_one(db)
    .await
}

pub async fn update_group(
    db: &PgPool,
    id: Uuid,
    name: &str,
    position: Option<i32>,
) -> Result<Option<ConnectionGroup>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH g AS (UPDATE connection_groups SET name = $2, position = COALESCE($3, position) \
                    WHERE id = $1 RETURNING *) \
         SELECT {GROUP_COLUMNS} FROM g"
    ))
    .bind(id)
    .bind(name)
    .bind(position)
    .fetch_optional(db)
    .await
}

pub async fn delete_group(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM connection_groups WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn next_group_position(db: &PgPool, workspace_id: Uuid) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(max(position) + 1, 0) FROM connection_groups WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(db)
    .await
}

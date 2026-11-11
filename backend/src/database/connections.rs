//! Repository for `database_connections`. Every query is scoped by `user_id`
//! so ownership is enforced in SQL, not only in application code.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::connection::ConnectionRecord;

const COLUMNS: &str = "id, user_id, name, database_type, host, port, database_name, username, \
     ssl_mode, read_only, encrypted_credentials, created_at, updated_at, last_connected_at";

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
}

pub async fn list_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<ConnectionRecord>, sqlx::Error> {
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

pub async fn insert(db: &PgPool, c: &NewConnection<'_>) -> Result<ConnectionRecord, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO database_connections \
            (id, user_id, name, database_type, host, port, database_name, username, ssl_mode, read_only, encrypted_credentials) \
         VALUES ($1, $2, $3, 'postgresql', $4, $5, $6, $7, $8, $9, $10) \
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
    .fetch_one(db)
    .await
}

/// Replaces all mutable fields. Callers merge partial updates first.
pub async fn update(db: &PgPool, c: &NewConnection<'_>) -> Result<Option<ConnectionRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "UPDATE database_connections SET \
            name = $3, host = $4, port = $5, database_name = $6, username = $7, \
            ssl_mode = $8, read_only = $9, encrypted_credentials = $10 \
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

pub async fn touch_last_connected(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE database_connections SET last_connected_at = now() WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

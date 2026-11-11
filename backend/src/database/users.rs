use sqlx::PgPool;
use uuid::Uuid;

use crate::models::user::UserRecord;

const COLUMNS: &str = "id, email, password_hash, display_name, created_at";

pub async fn insert(
    db: &PgPool,
    id: Uuid,
    email: &str,
    password_hash: &str,
    display_name: Option<&str>,
) -> Result<UserRecord, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .fetch_one(db)
    .await
}

pub async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<UserRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM users WHERE lower(email) = lower($1)"
    ))
    .bind(email)
    .fetch_optional(db)
    .await
}

pub async fn find_by_id(db: &PgPool, id: Uuid) -> Result<Option<UserRecord>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn update_password_hash(
    db: &PgPool,
    id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(password_hash)
        .execute(db)
        .await?;
    Ok(())
}

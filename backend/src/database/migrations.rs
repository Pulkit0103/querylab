//! Migration projects, scripts and run history (metadata database). Every
//! statement is scoped by `user_id`.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::migrations::{Migration, MigrationRun, Project};

pub async fn list_projects(db: &PgPool, user_id: Uuid) -> Result<Vec<Project>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.name, p.created_at, \
                (SELECT count(*) FROM migrations m WHERE m.project_id = p.id)::bigint AS migrations \
         FROM migration_projects p WHERE p.user_id = $1 ORDER BY lower(p.name), p.id",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn get_project(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.name, p.created_at, \
                (SELECT count(*) FROM migrations m WHERE m.project_id = p.id)::bigint AS migrations \
         FROM migration_projects p WHERE p.id = $1 AND p.user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn insert_project(db: &PgPool, user_id: Uuid, name: &str) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_projects (id, user_id, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(user_id)
        .bind(name)
        .execute(db)
        .await?;
    Ok(id)
}

pub async fn delete_project(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM migration_projects WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Applied somewhere: the latest successful run on some connection was `up`.
const LOCKED: &str = "EXISTS (SELECT 1 FROM ( \
         SELECT DISTINCT ON (r.connection_id) r.direction FROM migration_runs r \
         WHERE r.migration_id = m.id AND r.status = 'success' \
         ORDER BY r.connection_id, r.started_at DESC) last WHERE last.direction = 'up') AS locked";

pub async fn list_migrations(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<Vec<Migration>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT m.id, m.project_id, m.version, m.name, m.up_sql, m.down_sql, m.checksum, \
                m.created_at, m.updated_at, {LOCKED} \
         FROM migrations m WHERE m.project_id = $1 AND m.user_id = $2 ORDER BY m.version"
    ))
    .bind(project_id)
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn get_migration(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<Migration>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT m.id, m.project_id, m.version, m.name, m.up_sql, m.down_sql, m.checksum, \
                m.created_at, m.updated_at, {LOCKED} \
         FROM migrations m WHERE m.id = $1 AND m.user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub struct NewMigration<'a> {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub version: i64,
    pub name: &'a str,
    pub up_sql: &'a str,
    pub down_sql: Option<&'a str>,
    pub checksum: &'a str,
}

pub async fn insert_migration(db: &PgPool, m: &NewMigration<'_>) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO migrations (id, project_id, user_id, version, name, up_sql, down_sql, checksum) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(m.project_id)
    .bind(m.user_id)
    .bind(m.version)
    .bind(m.name)
    .bind(m.up_sql)
    .bind(m.down_sql)
    .bind(m.checksum)
    .execute(db)
    .await?;
    Ok(id)
}

pub async fn update_migration(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    m: &NewMigration<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE migrations SET version = $3, name = $4, up_sql = $5, down_sql = $6, checksum = $7, \
                updated_at = now() \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .bind(m.version)
    .bind(m.name)
    .bind(m.up_sql)
    .bind(m.down_sql)
    .bind(m.checksum)
    .execute(db)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn delete_migration(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM migrations WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

pub struct NewRun<'a> {
    pub user_id: Uuid,
    pub migration_id: Uuid,
    pub connection_id: Uuid,
    pub direction: &'a str,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub duration_ms: i64,
}

pub async fn insert_run(db: &PgPool, r: &NewRun<'_>) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO migration_runs \
            (id, migration_id, user_id, connection_id, direction, status, error_message, duration_ms) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(r.migration_id)
    .bind(r.user_id)
    .bind(r.connection_id)
    .bind(r.direction)
    .bind(r.status)
    .bind(r.error_message)
    .bind(r.duration_ms)
    .execute(db)
    .await?;
    Ok(id)
}

pub async fn list_runs(
    db: &PgPool,
    user_id: Uuid,
    project_id: Option<Uuid>,
    connection_id: Option<Uuid>,
) -> Result<Vec<MigrationRun>, sqlx::Error> {
    sqlx::query_as(
        "SELECT r.id, r.migration_id, m.project_id, m.version, m.name, r.connection_id, \
                c.name AS connection_name, r.direction, r.status, r.error_message, r.duration_ms, r.started_at \
         FROM migration_runs r \
         JOIN migrations m ON m.id = r.migration_id \
         LEFT JOIN database_connections c ON c.id = r.connection_id \
         WHERE r.user_id = $1 AND ($2::uuid IS NULL OR m.project_id = $2) \
           AND ($3::uuid IS NULL OR r.connection_id = $3) \
         ORDER BY r.started_at DESC, r.id LIMIT 200",
    )
    .bind(user_id)
    .bind(project_id)
    .bind(connection_id)
    .fetch_all(db)
    .await
}

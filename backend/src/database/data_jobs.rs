//! Durable records of imports, exports and mock data runs (metadata
//! database). Every statement is scoped by `user_id`.

use sqlx::{types::Json, PgPool};
use uuid::Uuid;

use crate::models::data_jobs::{JobKind, JobRecord, JobView};

/// Inserts the row for a job that is starting.
pub async fn insert_running(db: &PgPool, user_id: Uuid, job: &JobView) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO data_jobs (id, user_id, connection_id, kind, status, target) \
         VALUES ($1, $2, $3, $4, 'running', $5) ON CONFLICT (id) DO NOTHING",
    )
    .bind(job.id)
    .bind(user_id)
    .bind(job.connection_id)
    .bind(job.kind.as_str())
    .bind(job.target.as_deref())
    .execute(db)
    .await?;
    Ok(())
}

/// Stores the final state of a job (inserting it if it was never started,
/// e.g. an export).
pub async fn finish(db: &PgPool, user_id: Uuid, job: &JobView) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO data_jobs (id, user_id, connection_id, kind, status, target, \
                                rows_processed, rows_failed, report, started_at, finished_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, COALESCE($10, now()), now()) \
         ON CONFLICT (id) DO UPDATE SET status = EXCLUDED.status, \
             rows_processed = EXCLUDED.rows_processed, rows_failed = EXCLUDED.rows_failed, \
             report = EXCLUDED.report, finished_at = EXCLUDED.finished_at \
         WHERE data_jobs.user_id = EXCLUDED.user_id",
    )
    .bind(job.id)
    .bind(user_id)
    .bind(job.connection_id)
    .bind(job.kind.as_str())
    .bind(job.status.as_str())
    .bind(job.target.as_deref())
    .bind(job.rows_processed as i64)
    .bind(job.rows_failed as i64)
    .bind(Json(job))
    .bind(job.started_at)
    .execute(db)
    .await?;
    Ok(())
}

const SELECT: &str = "SELECT j.id, j.connection_id, c.name AS connection_name, j.kind, j.status, \
            j.target, j.rows_processed, j.rows_failed, j.started_at, j.finished_at \
     FROM data_jobs j LEFT JOIN database_connections c ON c.id = j.connection_id";

pub async fn list(
    db: &PgPool,
    user_id: Uuid,
    kind: Option<JobKind>,
    limit: i64,
) -> Result<Vec<JobRecord>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{SELECT} WHERE j.user_id = $1 AND ($2::text IS NULL OR j.kind = $2) \
         ORDER BY j.started_at DESC, j.id LIMIT $3"
    ))
    .bind(user_id)
    .bind(kind.map(JobKind::as_str))
    .bind(limit)
    .fetch_all(db)
    .await
}

#[derive(sqlx::FromRow)]
pub struct StoredJob {
    #[sqlx(flatten)]
    pub record: JobRecord,
    pub report: Option<Json<JobView>>,
}

/// A job with its stored report.
pub async fn get(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<StoredJob>, sqlx::Error> {
    sqlx::query_as(
        "SELECT j.id, j.connection_id, c.name AS connection_name, j.kind, j.status, \
                j.target, j.rows_processed, j.rows_failed, j.started_at, j.finished_at, j.report \
         FROM data_jobs j LEFT JOIN database_connections c ON c.id = j.connection_id \
         WHERE j.id = $1 AND j.user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Jobs left `running` by a previous process can never finish.
pub async fn fail_interrupted(db: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE data_jobs SET status = 'failed', finished_at = now(), \
             report = jsonb_set(COALESCE(report, '{}'::jsonb), '{error}', \
                                to_jsonb('interrupted by a server restart'::text)) \
         WHERE status = 'running'",
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

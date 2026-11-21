//! Performance history by fingerprint and saved plans. Scoped by `user_id`.

use serde_json::Value;
use sqlx::{types::Json, PgPool};
use uuid::Uuid;

use crate::models::{
    library::HistoryEntry,
    performance::{FingerprintStats, SavedPlan, SavedPlanSummary, TimelinePoint, MAX_FINGERPRINTS},
};

/// Aggregates over editor queries in the last `days`; timings use
/// successful executions only.
const STATS_AGGREGATES: &str = "count(*)::bigint AS executions, \
     count(*) FILTER (WHERE h.status <> 'success')::bigint AS errors, \
     (avg(h.execution_time_ms) FILTER (WHERE h.status = 'success'))::float8 AS avg_ms, \
     (percentile_cont(0.5) WITHIN GROUP (ORDER BY h.execution_time_ms) FILTER (WHERE h.status = 'success'))::float8 AS p50_ms, \
     (percentile_cont(0.95) WITHIN GROUP (ORDER BY h.execution_time_ms) FILTER (WHERE h.status = 'success'))::float8 AS p95_ms, \
     (max(h.execution_time_ms) FILTER (WHERE h.status = 'success'))::bigint AS max_ms";

const RANGE_FILTER: &str = "h.user_id = $1 AND h.kind = 'query' AND h.fingerprint IS NOT NULL \
     AND h.executed_at >= now() - make_interval(days => $2) \
     AND ($3::uuid IS NULL OR h.connection_id = $3)";

/// The latest statement of a fingerprint group (with its connection).
const LATEST_SAMPLE: &str = "LEFT JOIN LATERAL ( \
         SELECT left(l.sql_content, 2000) AS sample_sql, l.connection_id, c.name AS connection_name \
         FROM query_history l LEFT JOIN database_connections c ON c.id = l.connection_id \
         WHERE l.user_id = $1 AND l.fingerprint = g.fingerprint \
           AND ($3::uuid IS NULL OR l.connection_id = $3) \
         ORDER BY l.executed_at DESC LIMIT 1) s ON true";

pub async fn list_fingerprints(
    db: &PgPool,
    user_id: Uuid,
    days: i32,
    connection_id: Option<Uuid>,
) -> Result<Vec<FingerprintStats>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH g AS ( \
             SELECT h.fingerprint, {STATS_AGGREGATES}, \
                    (sum(h.execution_time_ms) FILTER (WHERE h.status = 'success'))::bigint AS total_ms, \
                    max(h.executed_at) AS last_executed_at \
             FROM query_history h WHERE {RANGE_FILTER} \
             GROUP BY h.fingerprint \
             ORDER BY total_ms DESC NULLS LAST, executions DESC \
             LIMIT {MAX_FINGERPRINTS}) \
         SELECT g.*, s.sample_sql, s.connection_id, s.connection_name FROM g {LATEST_SAMPLE} \
         ORDER BY g.total_ms DESC NULLS LAST, g.executions DESC"
    ))
    .bind(user_id)
    .bind(days)
    .bind(connection_id)
    .fetch_all(db)
    .await
}

pub async fn fingerprint_stats(
    db: &PgPool,
    user_id: Uuid,
    fingerprint: i64,
    days: i32,
    connection_id: Option<Uuid>,
) -> Result<Option<FingerprintStats>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH g AS ( \
             SELECT $4::bigint AS fingerprint, {STATS_AGGREGATES}, \
                    (sum(h.execution_time_ms) FILTER (WHERE h.status = 'success'))::bigint AS total_ms, \
                    max(h.executed_at) AS last_executed_at \
             FROM query_history h WHERE {RANGE_FILTER} AND h.fingerprint = $4) \
         SELECT g.*, s.sample_sql, s.connection_id, s.connection_name FROM g {LATEST_SAMPLE} \
         WHERE g.executions > 0"
    ))
    .bind(user_id)
    .bind(days)
    .bind(connection_id)
    .bind(fingerprint)
    .fetch_optional(db)
    .await
}

pub async fn fingerprint_timeline(
    db: &PgPool,
    user_id: Uuid,
    fingerprint: i64,
    days: i32,
    connection_id: Option<Uuid>,
) -> Result<Vec<TimelinePoint>, sqlx::Error> {
    sqlx::query_as(&format!(
        "WITH per_day AS ( \
             SELECT (h.executed_at AT TIME ZONE 'UTC')::date AS day, {STATS_AGGREGATES} \
             FROM query_history h WHERE {RANGE_FILTER} AND h.fingerprint = $4 \
             GROUP BY 1) \
         SELECT d.day::date AS day, COALESCE(p.executions, 0) AS executions, \
                COALESCE(p.errors, 0) AS errors, p.p50_ms, p.p95_ms, p.max_ms \
         FROM generate_series((now() AT TIME ZONE 'UTC')::date - ($2 - 1), \
                              (now() AT TIME ZONE 'UTC')::date, interval '1 day') AS d(day) \
         LEFT JOIN per_day p ON p.day = d.day::date \
         ORDER BY 1"
    ))
    .bind(user_id)
    .bind(days)
    .bind(connection_id)
    .bind(fingerprint)
    .fetch_all(db)
    .await
}

pub async fn recent_with_fingerprint(
    db: &PgPool,
    user_id: Uuid,
    fingerprint: i64,
    connection_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    sqlx::query_as(
        "SELECT h.id, h.connection_id, c.name AS connection_name, h.sql_content, \
                h.execution_time_ms, h.status, h.error_message, h.row_count, h.executed_at, h.pinned, h.kind \
         FROM query_history h LEFT JOIN database_connections c ON c.id = h.connection_id \
         WHERE h.user_id = $1 AND h.fingerprint = $2 AND ($3::uuid IS NULL OR h.connection_id = $3) \
         ORDER BY h.executed_at DESC, h.id DESC LIMIT $4",
    )
    .bind(user_id)
    .bind(fingerprint)
    .bind(connection_id)
    .bind(limit)
    .fetch_all(db)
    .await
}

// ---------------------------------------------------------------------------
// Saved plans
// ---------------------------------------------------------------------------

const PLAN_SUMMARY: &str = "SELECT p.id, p.name, p.connection_id, c.name AS connection_name, \
       left(p.sql_content, 300) AS sql_preview, p.analyzed, \
       CASE WHEN jsonb_typeof(p.plan->'execution_time_ms') = 'number' \
            THEN (p.plan->>'execution_time_ms')::float8 END AS execution_time_ms, \
       CASE WHEN jsonb_typeof(p.plan->'plan'->'Total Cost') = 'number' \
            THEN (p.plan->'plan'->>'Total Cost')::float8 END AS total_cost, \
       p.created_at \
     FROM saved_plans p LEFT JOIN database_connections c ON c.id = p.connection_id";

pub async fn list_plans(db: &PgPool, user_id: Uuid) -> Result<Vec<SavedPlanSummary>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{PLAN_SUMMARY} WHERE p.user_id = $1 ORDER BY p.created_at DESC, p.id"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn count_plans(db: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM saved_plans WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await
}

pub struct NewPlan<'a> {
    pub user_id: Uuid,
    pub connection_id: Option<Uuid>,
    pub name: &'a str,
    pub sql: &'a str,
    pub plan: &'a Value,
    pub analyzed: bool,
}

pub async fn insert_plan(db: &PgPool, p: &NewPlan<'_>) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO saved_plans (id, user_id, connection_id, name, sql_content, plan, analyzed) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(p.user_id)
    .bind(p.connection_id)
    .bind(p.name)
    .bind(p.sql)
    .bind(Json(p.plan))
    .bind(p.analyzed)
    .execute(db)
    .await?;
    Ok(id)
}

pub async fn get_plan(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<SavedPlan>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id, p.name, p.connection_id, c.name AS connection_name, p.sql_content, p.plan, \
                p.analyzed, p.created_at \
         FROM saved_plans p LEFT JOIN database_connections c ON c.id = p.connection_id \
         WHERE p.id = $1 AND p.user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn delete_plan(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM saved_plans WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

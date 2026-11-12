//! Repositories for query history, saved queries, collections and stats.
//! Every statement is parameterised and scoped by `user_id`.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::library::{
    Collection, ConnectionUsage, DailyStats, HistoryEntry, HistoryStatus, SavedQuery, StatsTotals,
};

/// Escapes LIKE/ILIKE wildcards so user search text matches literally.
pub fn like_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

const HISTORY_COLUMNS: &str = "h.id, h.connection_id, c.name AS connection_name, h.sql_content, \
     h.execution_time_ms, h.status, h.error_message, h.row_count, h.executed_at";

pub struct NewHistory<'a> {
    pub user_id: Uuid,
    pub connection_id: Uuid,
    pub sql: &'a str,
    pub execution_time_ms: i64,
    pub status: HistoryStatus,
    pub error_message: Option<&'a str>,
    pub row_count: Option<i64>,
}

pub async fn insert_history(db: &PgPool, h: &NewHistory<'_>) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO query_history \
            (id, user_id, connection_id, sql_content, execution_time_ms, status, error_message, row_count) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(h.user_id)
    .bind(h.connection_id)
    .bind(h.sql)
    .bind(h.execution_time_ms.max(0))
    .bind(h.status.as_str())
    .bind(h.error_message)
    .bind(h.row_count)
    .execute(db)
    .await?;
    Ok(id)
}

pub struct HistoryFilter<'a> {
    pub connection_id: Option<Uuid>,
    pub status: Option<HistoryStatus>,
    pub search: Option<&'a str>,
    pub before: Option<Uuid>,
    pub limit: i64,
}

pub async fn list_history(
    db: &PgPool,
    user_id: Uuid,
    f: &HistoryFilter<'_>,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {HISTORY_COLUMNS} \
         FROM query_history h LEFT JOIN database_connections c ON c.id = h.connection_id \
         WHERE h.user_id = $1 \
           AND ($2::uuid IS NULL OR h.connection_id = $2) \
           AND ($3::text IS NULL OR h.status = $3) \
           AND ($4::text IS NULL OR h.sql_content ILIKE $4) \
           AND ($5::uuid IS NULL OR (h.executed_at, h.id) < \
                (SELECT executed_at, id FROM query_history WHERE id = $5 AND user_id = $1)) \
         ORDER BY h.executed_at DESC, h.id DESC \
         LIMIT $6"
    ))
    .bind(user_id)
    .bind(f.connection_id)
    .bind(f.status.map(|s| s.as_str()))
    .bind(f.search.map(like_pattern))
    .bind(f.before)
    .bind(f.limit)
    .fetch_all(db)
    .await
}

pub async fn get_history(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<HistoryEntry>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {HISTORY_COLUMNS} \
         FROM query_history h LEFT JOIN database_connections c ON c.id = h.connection_id \
         WHERE h.user_id = $1 AND h.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await
}

pub async fn delete_history(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM query_history WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(id)
            .execute(db)
            .await?
            .rows_affected()
            == 1,
    )
}

pub async fn clear_history(db: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM query_history WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected())
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

pub async fn list_collections(db: &PgPool, user_id: Uuid) -> Result<Vec<Collection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.id, c.name, count(s.id)::bigint AS query_count, c.created_at, c.updated_at \
         FROM query_collections c LEFT JOIN saved_queries s ON s.collection_id = c.id \
         WHERE c.user_id = $1 \
         GROUP BY c.id ORDER BY lower(c.name)",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn insert_collection(
    db: &PgPool,
    user_id: Uuid,
    name: &str,
) -> Result<Collection, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO query_collections (id, user_id, name) VALUES ($1, $2, $3) \
         RETURNING id, name, 0::bigint AS query_count, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(name)
    .fetch_one(db)
    .await
}

pub async fn rename_collection(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    name: &str,
) -> Result<Option<Collection>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE query_collections SET name = $3 WHERE id = $1 AND user_id = $2 \
         RETURNING id, name, \
           (SELECT count(*) FROM saved_queries WHERE collection_id = $1)::bigint AS query_count, \
           created_at, updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .fetch_optional(db)
    .await
}

pub async fn delete_collection(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM query_collections WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(db)
            .await?
            .rows_affected()
            == 1,
    )
}

// ---------------------------------------------------------------------------
// Saved queries
// ---------------------------------------------------------------------------

const SAVED_COLUMNS: &str =
    "id, name, description, sql_content, connection_id, collection_id, created_at, updated_at";

pub struct SavedQueryRow<'a> {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub sql_content: &'a str,
    pub connection_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
}

pub async fn list_saved(
    db: &PgPool,
    user_id: Uuid,
    collection_id: Option<Uuid>,
    search: Option<&str>,
) -> Result<Vec<SavedQuery>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SAVED_COLUMNS} FROM saved_queries \
         WHERE user_id = $1 \
           AND ($2::uuid IS NULL OR collection_id = $2) \
           AND ($3::text IS NULL OR name ILIKE $3 OR description ILIKE $3 OR sql_content ILIKE $3) \
         ORDER BY updated_at DESC \
         LIMIT 1000"
    ))
    .bind(user_id)
    .bind(collection_id)
    .bind(search.map(like_pattern))
    .fetch_all(db)
    .await
}

pub async fn get_saved(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<SavedQuery>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SAVED_COLUMNS} FROM saved_queries WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn insert_saved(db: &PgPool, q: &SavedQueryRow<'_>) -> Result<SavedQuery, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO saved_queries (id, user_id, name, description, sql_content, connection_id, collection_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {SAVED_COLUMNS}"
    ))
    .bind(q.id)
    .bind(q.user_id)
    .bind(q.name)
    .bind(q.description)
    .bind(q.sql_content)
    .bind(q.connection_id)
    .bind(q.collection_id)
    .fetch_one(db)
    .await
}

pub async fn update_saved(
    db: &PgPool,
    q: &SavedQueryRow<'_>,
) -> Result<Option<SavedQuery>, sqlx::Error> {
    sqlx::query_as(&format!(
        "UPDATE saved_queries SET name = $3, description = $4, sql_content = $5, \
            connection_id = $6, collection_id = $7 \
         WHERE id = $1 AND user_id = $2 RETURNING {SAVED_COLUMNS}"
    ))
    .bind(q.id)
    .bind(q.user_id)
    .bind(q.name)
    .bind(q.description)
    .bind(q.sql_content)
    .bind(q.connection_id)
    .bind(q.collection_id)
    .fetch_optional(db)
    .await
}

pub async fn delete_saved(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM saved_queries WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(db)
            .await?
            .rows_affected()
            == 1,
    )
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

pub async fn stats_totals(
    db: &PgPool,
    user_id: Uuid,
    days: i32,
) -> Result<StatsTotals, sqlx::Error> {
    sqlx::query_as(
        "SELECT count(*)::bigint AS executions, \
                count(*) FILTER (WHERE status = 'success')::bigint AS successful, \
                count(*) FILTER (WHERE status <> 'success')::bigint AS failed, \
                avg(execution_time_ms)::float8 AS avg_execution_ms, \
                percentile_cont(0.95) WITHIN GROUP (ORDER BY execution_time_ms)::float8 AS p95_execution_ms, \
                (SELECT count(*) FROM saved_queries WHERE user_id = $1)::bigint AS saved_queries, \
                (SELECT count(*) FROM database_connections WHERE user_id = $1)::bigint AS connections \
         FROM query_history \
         WHERE user_id = $1 AND executed_at >= now() - make_interval(days => $2)",
    )
    .bind(user_id)
    .bind(days)
    .fetch_one(db)
    .await
}

pub async fn stats_daily(
    db: &PgPool,
    user_id: Uuid,
    days: i32,
) -> Result<Vec<DailyStats>, sqlx::Error> {
    sqlx::query_as(
        "SELECT d::date AS day, \
                count(h.id) FILTER (WHERE h.status = 'success')::bigint AS successful, \
                count(h.id) FILTER (WHERE h.status <> 'success')::bigint AS failed, \
                avg(h.execution_time_ms)::float8 AS avg_execution_ms \
         FROM generate_series((now() - make_interval(days => $2 - 1))::date, now()::date, interval '1 day') AS d \
         LEFT JOIN query_history h ON h.user_id = $1 AND h.executed_at::date = d::date \
         GROUP BY d ORDER BY d",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(db)
    .await
}

pub async fn stats_by_connection(
    db: &PgPool,
    user_id: Uuid,
    days: i32,
) -> Result<Vec<ConnectionUsage>, sqlx::Error> {
    sqlx::query_as(
        "SELECT h.connection_id, c.name AS connection_name, count(*)::bigint AS executions \
         FROM query_history h LEFT JOIN database_connections c ON c.id = h.connection_id \
         WHERE h.user_id = $1 AND h.executed_at >= now() - make_interval(days => $2) \
         GROUP BY h.connection_id, c.name ORDER BY executions DESC LIMIT 6",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(db)
    .await
}

pub async fn stats_slowest(
    db: &PgPool,
    user_id: Uuid,
    days: i32,
) -> Result<Vec<HistoryEntry>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {HISTORY_COLUMNS} \
         FROM query_history h LEFT JOIN database_connections c ON c.id = h.connection_id \
         WHERE h.user_id = $1 AND h.status = 'success' \
           AND h.executed_at >= now() - make_interval(days => $2) \
         ORDER BY h.execution_time_ms DESC LIMIT 5"
    ))
    .bind(user_id)
    .bind(days)
    .fetch_all(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::like_pattern;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("orders"), "%orders%");
        assert_eq!(like_pattern("100%_off\\"), "%100\\%\\_off\\\\%");
    }
}

//! Repositories for query history, saved queries, folders, tags and stats.
//! Every statement is parameterised and scoped by `user_id`.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    models::library::{
        Collection, ConnectionUsage, DailyStats, HistoryEntry, HistoryKind, HistoryStatus,
        SavedQuery, StatsTotals, TagCount, MAX_FOLDER_DEPTH,
    },
    utils::fingerprint::fingerprint,
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
     h.execution_time_ms, h.status, h.error_message, h.row_count, h.executed_at, h.pinned, h.kind";

pub struct NewHistory<'a> {
    pub user_id: Uuid,
    pub connection_id: Uuid,
    pub sql: &'a str,
    pub execution_time_ms: i64,
    pub status: HistoryStatus,
    pub error_message: Option<&'a str>,
    pub row_count: Option<i64>,
    pub kind: HistoryKind,
}

pub async fn insert_history(db: &PgPool, h: &NewHistory<'_>) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO query_history \
            (id, user_id, connection_id, sql_content, execution_time_ms, status, error_message, row_count, kind, fingerprint) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(h.user_id)
    .bind(h.connection_id)
    .bind(h.sql)
    .bind(h.execution_time_ms.max(0))
    .bind(h.status.as_str())
    .bind(h.error_message)
    .bind(h.row_count)
    .bind(h.kind.as_str())
    .bind(fingerprint(h.sql))
    .execute(db)
    .await?;
    Ok(id)
}

/// Fingerprints up to `batch` history rows recorded before fingerprints
/// existed. Returns how many rows were updated (0 when done).
pub async fn backfill_fingerprints(db: &PgPool, batch: i64) -> Result<u64, sqlx::Error> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, sql_content FROM query_history WHERE fingerprint IS NULL \
         ORDER BY executed_at DESC LIMIT $1",
    )
    .bind(batch)
    .fetch_all(db)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let ids: Vec<Uuid> = rows.iter().map(|(id, _)| *id).collect();
    let fps: Vec<i64> = rows.iter().map(|(_, sql)| fingerprint(sql)).collect();
    let done = sqlx::query(
        "UPDATE query_history h SET fingerprint = v.fp \
         FROM unnest($1::uuid[], $2::bigint[]) AS v(id, fp) \
         WHERE h.id = v.id AND h.fingerprint IS NULL",
    )
    .bind(&ids)
    .bind(&fps)
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

#[derive(Default)]
pub struct HistoryFilter<'a> {
    pub connection_id: Option<Uuid>,
    pub status: Option<HistoryStatus>,
    pub kind: Option<HistoryKind>,
    pub search: Option<&'a str>,
    pub min_ms: Option<i64>,
    pub max_ms: Option<i64>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub pinned: Option<bool>,
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
           AND ($7::text IS NULL OR h.kind = $7) \
           AND ($8::bigint IS NULL OR h.execution_time_ms >= $8) \
           AND ($9::bigint IS NULL OR h.execution_time_ms <= $9) \
           AND ($10::timestamptz IS NULL OR h.executed_at >= $10) \
           AND ($11::timestamptz IS NULL OR h.executed_at < $11) \
           AND ($12::boolean IS NULL OR h.pinned = $12) \
         ORDER BY h.executed_at DESC, h.id DESC \
         LIMIT $6"
    ))
    .bind(user_id)
    .bind(f.connection_id)
    .bind(f.status.map(|s| s.as_str()))
    .bind(f.search.map(like_pattern))
    .bind(f.before)
    .bind(f.limit)
    .bind(f.kind.map(|k| k.as_str()))
    .bind(f.min_ms)
    .bind(f.max_ms)
    .bind(f.from)
    .bind(f.to)
    .bind(f.pinned)
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

pub async fn set_history_pinned(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    pinned: bool,
) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("UPDATE query_history SET pinned = $3 WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(id)
            .bind(pinned)
            .execute(db)
            .await?
            .rows_affected()
            == 1,
    )
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

/// Deletes one batch of unpinned history older than each owner's retention
/// setting. Returns the number of rows removed.
pub async fn prune_history_batch(db: &PgPool, batch: i64) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "DELETE FROM query_history WHERE id IN ( \
             SELECT h.id FROM query_history h JOIN users u ON u.id = h.user_id \
             WHERE NOT h.pinned \
               AND h.executed_at < now() - make_interval(days => u.history_retention_days) \
             LIMIT $1)",
    )
    .bind(batch)
    .execute(db)
    .await?
    .rows_affected())
}

// ---------------------------------------------------------------------------
// Folders (collections)
// ---------------------------------------------------------------------------

const COLLECTION_SELECT: &str = "SELECT c.id, c.name, c.parent_id, \
       (SELECT count(*) FROM saved_queries s WHERE s.collection_id = c.id)::bigint AS query_count, \
       c.created_at, c.updated_at \
     FROM query_collections c";

/// Why a folder could not be created or moved.
#[derive(Debug, PartialEq, Eq)]
pub enum FolderRejection {
    NotFound,
    ParentNotFound,
    /// The new parent is the folder itself or one of its descendants.
    Cycle,
    TooDeep,
}

pub async fn list_collections(db: &PgPool, user_id: Uuid) -> Result<Vec<Collection>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{COLLECTION_SELECT} WHERE c.user_id = $1 ORDER BY lower(c.name), c.id"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

async fn get_collection(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<Collection>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{COLLECTION_SELECT} WHERE c.user_id = $1 AND c.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

/// Serialises folder changes per user so concurrent moves cannot combine
/// into a cycle.
async fn lock_folders(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT id FROM query_collections WHERE user_id = $1 FOR UPDATE")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Depth of a folder (top level = 1), or `None` if it does not exist.
async fn folder_depth(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "WITH RECURSIVE up AS ( \
             SELECT id, parent_id, 1 AS depth FROM query_collections WHERE id = $2 AND user_id = $1 \
             UNION ALL \
             SELECT c.id, c.parent_id, up.depth + 1 FROM query_collections c \
             JOIN up ON c.id = up.parent_id \
             WHERE up.depth < 100 \
         ) SELECT max(depth) FROM up",
    )
    .bind(user_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await
}

/// A folder and all its descendants, with the height of the subtree.
async fn folder_subtree(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<(Vec<Uuid>, i32), sqlx::Error> {
    let rows: Vec<(Uuid, i32)> = sqlx::query_as(
        "WITH RECURSIVE down AS ( \
             SELECT id, 1 AS level FROM query_collections WHERE id = $2 AND user_id = $1 \
             UNION ALL \
             SELECT c.id, down.level + 1 FROM query_collections c \
             JOIN down ON c.parent_id = down.id \
             WHERE down.level < 100 \
         ) SELECT id, level FROM down",
    )
    .bind(user_id)
    .bind(id)
    .fetch_all(&mut **tx)
    .await?;
    let height = rows.iter().map(|(_, l)| *l).max().unwrap_or(0);
    Ok((rows.into_iter().map(|(id, _)| id).collect(), height))
}

pub async fn insert_collection(
    db: &PgPool,
    user_id: Uuid,
    name: &str,
    parent_id: Option<Uuid>,
) -> Result<Result<Collection, FolderRejection>, sqlx::Error> {
    let mut tx = db.begin().await?;
    lock_folders(&mut tx, user_id).await?;
    if let Some(parent) = parent_id {
        match folder_depth(&mut tx, user_id, parent).await? {
            None => return Ok(Err(FolderRejection::ParentNotFound)),
            Some(depth) if depth + 1 > MAX_FOLDER_DEPTH => {
                return Ok(Err(FolderRejection::TooDeep))
            }
            Some(_) => {}
        }
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO query_collections (id, user_id, name, parent_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(parent_id)
    .execute(&mut *tx)
    .await?;
    let created = get_collection(&mut tx, user_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    tx.commit().await?;
    Ok(Ok(created))
}

/// Renames and/or moves a folder. `parent` is `Some(None)` to move to the
/// top level and `None` to keep the current parent.
pub async fn update_collection(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    name: Option<&str>,
    parent: Option<Option<Uuid>>,
) -> Result<Result<Collection, FolderRejection>, sqlx::Error> {
    let mut tx = db.begin().await?;
    lock_folders(&mut tx, user_id).await?;
    let (subtree, height) = folder_subtree(&mut tx, user_id, id).await?;
    if subtree.is_empty() {
        return Ok(Err(FolderRejection::NotFound));
    }
    if let Some(Some(new_parent)) = parent {
        if subtree.contains(&new_parent) {
            return Ok(Err(FolderRejection::Cycle));
        }
        match folder_depth(&mut tx, user_id, new_parent).await? {
            None => return Ok(Err(FolderRejection::ParentNotFound)),
            Some(depth) if depth + height > MAX_FOLDER_DEPTH => {
                return Ok(Err(FolderRejection::TooDeep))
            }
            Some(_) => {}
        }
    }
    sqlx::query(
        "UPDATE query_collections \
         SET name = COALESCE($3, name), \
             parent_id = CASE WHEN $4 THEN $5 ELSE parent_id END \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(parent.is_some())
    .bind(parent.flatten())
    .execute(&mut *tx)
    .await?;
    let updated = get_collection(&mut tx, user_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    tx.commit().await?;
    Ok(Ok(updated))
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

const SAVED_COLUMNS: &str = "id, name, description, sql_content, connection_id, collection_id, \
     created_at, updated_at, pinned, tags";

pub struct SavedQueryRow<'a> {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub sql_content: &'a str,
    pub connection_id: Option<Uuid>,
    pub collection_id: Option<Uuid>,
    pub tags: &'a [String],
}

#[derive(Default)]
pub struct SavedFilter<'a> {
    pub collection_id: Option<Uuid>,
    pub search: Option<&'a str>,
    /// Queries must carry every one of these tags.
    pub tags: &'a [String],
    pub pinned: Option<bool>,
}

pub async fn list_saved(
    db: &PgPool,
    user_id: Uuid,
    f: &SavedFilter<'_>,
) -> Result<Vec<SavedQuery>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {SAVED_COLUMNS} FROM saved_queries \
         WHERE user_id = $1 \
           AND ($2::uuid IS NULL OR collection_id = $2) \
           AND ($3::text IS NULL OR name ILIKE $3 OR description ILIKE $3 OR sql_content ILIKE $3) \
           AND (cardinality($4::text[]) = 0 OR tags @> $4) \
           AND ($5::boolean IS NULL OR pinned = $5) \
         ORDER BY pinned DESC, updated_at DESC \
         LIMIT 1000"
    ))
    .bind(user_id)
    .bind(f.collection_id)
    .bind(f.search.map(like_pattern))
    .bind(f.tags)
    .bind(f.pinned)
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
        "INSERT INTO saved_queries (id, user_id, name, description, sql_content, connection_id, collection_id, tags) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {SAVED_COLUMNS}"
    ))
    .bind(q.id)
    .bind(q.user_id)
    .bind(q.name)
    .bind(q.description)
    .bind(q.sql_content)
    .bind(q.connection_id)
    .bind(q.collection_id)
    .bind(q.tags)
    .fetch_one(db)
    .await
}

pub async fn update_saved(
    db: &PgPool,
    q: &SavedQueryRow<'_>,
) -> Result<Option<SavedQuery>, sqlx::Error> {
    sqlx::query_as(&format!(
        "UPDATE saved_queries SET name = $3, description = $4, sql_content = $5, \
            connection_id = $6, collection_id = $7, tags = $8 \
         WHERE id = $1 AND user_id = $2 RETURNING {SAVED_COLUMNS}"
    ))
    .bind(q.id)
    .bind(q.user_id)
    .bind(q.name)
    .bind(q.description)
    .bind(q.sql_content)
    .bind(q.connection_id)
    .bind(q.collection_id)
    .bind(q.tags)
    .fetch_optional(db)
    .await
}

pub async fn set_saved_pinned(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    pinned: bool,
) -> Result<Option<SavedQuery>, sqlx::Error> {
    // The touch trigger leaves `updated_at` alone: pinning is not an edit.
    sqlx::query_as(&format!(
        "UPDATE saved_queries SET pinned = $3 \
         WHERE id = $1 AND user_id = $2 RETURNING {SAVED_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(pinned)
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

pub async fn list_tags(db: &PgPool, user_id: Uuid) -> Result<Vec<TagCount>, sqlx::Error> {
    sqlx::query_as(
        "SELECT t AS tag, count(*)::bigint AS count \
         FROM saved_queries, unnest(tags) AS t \
         WHERE user_id = $1 \
         GROUP BY t ORDER BY count(*) DESC, t \
         LIMIT 500",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
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

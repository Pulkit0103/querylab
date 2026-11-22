//! Query history, saved queries, collections and usage statistics.

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    database::library::{
        self as repo, FolderRejection, HistoryFilter, NewHistory, SavedFilter, SavedQueryRow,
    },
    errors::{AppError, AppResult},
    models::{
        library::{
            normalize_tags, Collection, CreateSavedQuery, HistoryEntry, HistoryKind, HistoryPage,
            HistoryQuery, HistoryStatus, SavedQuery, SavedQueryFilter, StatsResponse, TagCount,
            UpdateCollectionRequest, UpdateSavedQuery, MAX_FOLDER_DEPTH,
        },
        query::QueryResult,
    },
    services::audit::{self, AuditEvent},
    state::AppState,
};

/// How often expired history is pruned, and how much per statement.
const RETENTION_INTERVAL: Duration = Duration::from_secs(3600);
const RETENTION_BATCH: i64 = 5000;

/// Deletes unpinned history older than each user's retention setting, in
/// batches so a large backlog never holds long locks.
pub async fn prune_history(db: &PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let removed = repo::prune_history_batch(db, RETENTION_BATCH).await?;
        total += removed;
        if removed < RETENTION_BATCH as u64 {
            return Ok(total);
        }
    }
}

const FINGERPRINT_BATCH: i64 = 1000;

/// Fingerprints history recorded before fingerprints existed. New rows are
/// fingerprinted on insert, so this only has work after an upgrade.
pub async fn backfill_fingerprints(db: &PgPool) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let done = repo::backfill_fingerprints(db, FINGERPRINT_BATCH).await?;
        total += done;
        if done == 0 {
            return Ok(total);
        }
    }
}

/// Runs [`backfill_fingerprints`] once, then [`prune_history`] periodically.
pub fn spawn_history_retention(db: PgPool) {
    tokio::spawn(async move {
        match backfill_fingerprints(&db).await {
            Ok(n) if n > 0 => tracing::info!(updated = n, "history fingerprints backfilled"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "fingerprint backfill failed"),
        }
        let mut interval = tokio::time::interval(RETENTION_INTERVAL);
        loop {
            interval.tick().await;
            match prune_history(&db).await {
                Ok(n) if n > 0 => tracing::info!(removed = n, "expired history pruned"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "history pruning failed"),
            }
        }
    });
}

fn folder_error(rejection: FolderRejection) -> AppError {
    match rejection {
        FolderRejection::NotFound => AppError::NotFound("collection"),
        FolderRejection::ParentNotFound => AppError::NotFound("parent folder"),
        FolderRejection::Cycle => AppError::validation(
            "parent_id",
            "a folder cannot be moved into itself or one of its subfolders",
        ),
        FolderRejection::TooDeep => AppError::validation(
            "parent_id",
            format!("folders can be nested at most {MAX_FOLDER_DEPTH} levels deep"),
        ),
    }
}

const MAX_ERROR_MESSAGE_CHARS: usize = 2000;
/// Matches the `query_history_sql_length` check constraint.
const MAX_HISTORY_SQL_BYTES: usize = 1_048_576;

pub struct LibraryService<'a> {
    state: &'a AppState,
}

/// Saved-query references must belong to the same user; the composite
/// foreign keys enforce this, and violations are reported as 404s.
fn reference_error(err: sqlx::Error) -> AppError {
    match AppError::from(err) {
        AppError::BadRequest(_) => AppError::NotFound("connection or collection"),
        AppError::Conflict(_) => {
            AppError::Conflict("a collection with this name already exists".into())
        }
        other => other,
    }
}

impl<'a> LibraryService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    // -- history ----------------------------------------------------------

    /// Records an execution outcome. Failures to record are logged, never
    /// surfaced: history must not break query execution.
    pub async fn record_execution(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        sql: &str,
        outcome: &AppResult<QueryResult>,
    ) -> Option<Uuid> {
        let (status, time_ms, error_message, row_count) = match outcome {
            Ok(r) => (
                HistoryStatus::Success,
                r.execution_time_ms,
                None,
                Some(r.rows_affected.unwrap_or(r.row_count as u64) as i64),
            ),
            Err(AppError::Sql(info)) => (
                HistoryStatus::Error,
                info.execution_time_ms.unwrap_or(0),
                Some(info.message.clone()),
                None,
            ),
            Err(AppError::QueryTimeout {
                execution_time_ms, ..
            }) => (
                HistoryStatus::Timeout,
                *execution_time_ms,
                outcome.as_ref().err().map(|e| e.to_string()),
                None,
            ),
            Err(AppError::QueryCancelled { execution_time_ms }) => {
                (HistoryStatus::Cancelled, *execution_time_ms, None, None)
            }
            Err(e @ AppError::TargetUnavailable(_)) => {
                (HistoryStatus::Error, 0, Some(e.to_string()), None)
            }
            // Validation, auth, rate limiting etc. are not executions.
            Err(_) => return None,
        };
        let error_message: Option<String> =
            error_message.map(|m| m.chars().take(MAX_ERROR_MESSAGE_CHARS).collect());
        match repo::insert_history(
            &self.state.db,
            &NewHistory {
                user_id,
                connection_id,
                sql,
                execution_time_ms: time_ms as i64,
                status,
                error_message: error_message.as_deref(),
                row_count,
                kind: HistoryKind::Query,
            },
        )
        .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!(error = %e, "failed to record query history");
                None
            }
        }
    }

    /// Records a generated script (for example a Data Studio apply). The SQL
    /// is truncated to the history size limit. Failures are only logged.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_script(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        kind: HistoryKind,
        sql: &str,
        execution_time_ms: u64,
        row_count: Option<i64>,
        error: Option<&str>,
    ) -> Option<Uuid> {
        let mut end = sql.len().min(MAX_HISTORY_SQL_BYTES);
        while !sql.is_char_boundary(end) {
            end -= 1;
        }
        let error_message: Option<String> =
            error.map(|m| m.chars().take(MAX_ERROR_MESSAGE_CHARS).collect());
        let status = if error.is_some() {
            HistoryStatus::Error
        } else {
            HistoryStatus::Success
        };
        let id = repo::insert_history(
            &self.state.db,
            &NewHistory {
                user_id,
                connection_id,
                sql: &sql[..end],
                execution_time_ms: execution_time_ms as i64,
                status,
                error_message: error_message.as_deref(),
                row_count,
                kind,
            },
        )
        .await
        .map_err(|e| tracing::warn!(error = %e, "failed to record history"))
        .ok();
        // Every privileged script is also an audit event.
        let action = match kind {
            HistoryKind::DataApply => "data.applied",
            HistoryKind::Migration => "migration.run",
            HistoryKind::Import => "import.run",
            HistoryKind::MockData => "mock_data.run",
            HistoryKind::Admin => "session.signalled",
            HistoryKind::Query => "query.script",
        };
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user_id), action)
                .connection(connection_id)
                .detail(serde_json::json!({
                    "status": status.as_str(),
                    "rows": row_count,
                    "duration_ms": execution_time_ms,
                    "error": error_message,
                }))
                .sql(sql),
        )
        .await;
        id
    }

    pub async fn history(&self, user_id: Uuid, q: &HistoryQuery) -> AppResult<HistoryPage> {
        let limit = q.limit.unwrap_or(50).clamp(1, 200);
        let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let mut items = repo::list_history(
            &self.state.db,
            user_id,
            &HistoryFilter {
                connection_id: q.connection_id,
                status: q.status,
                kind: q.kind,
                search,
                min_ms: q.min_ms,
                max_ms: q.max_ms,
                from: q.from,
                to: q.to,
                pinned: q.pinned,
                before: q.before,
                limit: limit + 1,
            },
        )
        .await?;
        let next_cursor = if items.len() as i64 > limit {
            items.truncate(limit as usize);
            items.last().map(|h| h.id)
        } else {
            None
        };
        Ok(HistoryPage { items, next_cursor })
    }

    pub async fn history_entry(&self, user_id: Uuid, id: Uuid) -> AppResult<HistoryEntry> {
        repo::get_history(&self.state.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("history entry"))
    }

    pub async fn pin_history(
        &self,
        user_id: Uuid,
        id: Uuid,
        pinned: bool,
    ) -> AppResult<HistoryEntry> {
        if !repo::set_history_pinned(&self.state.db, user_id, id, pinned).await? {
            return Err(AppError::NotFound("history entry"));
        }
        self.history_entry(user_id, id).await
    }

    pub async fn delete_history(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_history(&self.state.db, user_id, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("history entry"))
        }
    }

    pub async fn clear_history(&self, user_id: Uuid) -> AppResult<u64> {
        Ok(repo::clear_history(&self.state.db, user_id).await?)
    }

    // -- collections ------------------------------------------------------

    pub async fn collections(&self, user_id: Uuid) -> AppResult<Vec<Collection>> {
        Ok(repo::list_collections(&self.state.db, user_id).await?)
    }

    pub async fn create_collection(
        &self,
        user_id: Uuid,
        name: &str,
        parent_id: Option<Uuid>,
    ) -> AppResult<Collection> {
        repo::insert_collection(&self.state.db, user_id, name.trim(), parent_id)
            .await
            .map_err(reference_error)?
            .map_err(folder_error)
    }

    /// Renames and/or moves a folder; moves are checked for cycles and depth.
    pub async fn update_collection(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &UpdateCollectionRequest,
    ) -> AppResult<Collection> {
        repo::update_collection(
            &self.state.db,
            user_id,
            id,
            req.name.as_deref().map(str::trim),
            req.parent_id,
        )
        .await
        .map_err(reference_error)?
        .map_err(folder_error)
    }

    pub async fn delete_collection(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_collection(&self.state.db, user_id, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("collection"))
        }
    }

    // -- saved queries ----------------------------------------------------

    pub async fn saved_queries(
        &self,
        user_id: Uuid,
        f: &SavedQueryFilter,
    ) -> AppResult<Vec<SavedQuery>> {
        let search = f.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let tags = f.tag_list();
        Ok(repo::list_saved(
            &self.state.db,
            user_id,
            &SavedFilter {
                collection_id: f.collection_id,
                search,
                tags: &tags,
                pinned: f.pinned,
            },
        )
        .await?)
    }

    pub async fn pin_saved(&self, user_id: Uuid, id: Uuid, pinned: bool) -> AppResult<SavedQuery> {
        repo::set_saved_pinned(&self.state.db, user_id, id, pinned)
            .await?
            .ok_or(AppError::NotFound("saved query"))
    }

    pub async fn tags(&self, user_id: Uuid) -> AppResult<Vec<TagCount>> {
        Ok(repo::list_tags(&self.state.db, user_id).await?)
    }

    pub async fn saved_query(&self, user_id: Uuid, id: Uuid) -> AppResult<SavedQuery> {
        repo::get_saved(&self.state.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("saved query"))
    }

    pub async fn create_saved(
        &self,
        user_id: Uuid,
        req: &CreateSavedQuery,
    ) -> AppResult<SavedQuery> {
        let description = req
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty());
        let tags = normalize_tags(&req.tags);
        repo::insert_saved(
            &self.state.db,
            &SavedQueryRow {
                id: Uuid::new_v4(),
                user_id,
                name: req.name.trim(),
                description,
                sql_content: &req.sql_content,
                connection_id: req.connection_id,
                collection_id: req.collection_id,
                tags: &tags,
            },
        )
        .await
        .map_err(reference_error)
    }

    pub async fn update_saved(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &UpdateSavedQuery,
    ) -> AppResult<SavedQuery> {
        let existing = self.saved_query(user_id, id).await?;
        let description = match &req.description {
            Some(d) => d.as_deref().map(str::trim).filter(|d| !d.is_empty()),
            None => existing.description.as_deref(),
        };
        let tags = match &req.tags {
            Some(tags) => normalize_tags(tags),
            None => existing.tags.clone(),
        };
        repo::update_saved(
            &self.state.db,
            &SavedQueryRow {
                id,
                user_id,
                name: req.name.as_deref().map(str::trim).unwrap_or(&existing.name),
                description,
                sql_content: req.sql_content.as_deref().unwrap_or(&existing.sql_content),
                connection_id: req.connection_id.unwrap_or(existing.connection_id),
                collection_id: req.collection_id.unwrap_or(existing.collection_id),
                tags: &tags,
            },
        )
        .await
        .map_err(reference_error)?
        .ok_or(AppError::NotFound("saved query"))
    }

    pub async fn delete_saved(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_saved(&self.state.db, user_id, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("saved query"))
        }
    }

    // -- stats ------------------------------------------------------------

    pub async fn stats(&self, user_id: Uuid, days: Option<i32>) -> AppResult<StatsResponse> {
        let days = days.unwrap_or(14).clamp(1, 90);
        let db = &self.state.db;
        let (totals, daily, by_connection, slowest) = tokio::try_join!(
            repo::stats_totals(db, user_id, days),
            repo::stats_daily(db, user_id, days),
            repo::stats_by_connection(db, user_id, days),
            repo::stats_slowest(db, user_id, days),
        )?;
        Ok(StatsResponse {
            days,
            totals,
            daily,
            by_connection,
            slowest,
        })
    }
}

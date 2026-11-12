//! Query history, saved queries, collections and usage statistics.

use uuid::Uuid;

use crate::{
    database::library::{self as repo, HistoryFilter, NewHistory, SavedQueryRow},
    errors::{AppError, AppResult},
    models::{
        library::{
            Collection, CreateSavedQuery, HistoryEntry, HistoryPage, HistoryQuery, HistoryStatus,
            SavedQuery, SavedQueryFilter, StatsResponse, UpdateSavedQuery,
        },
        query::QueryResult,
    },
    state::AppState,
};

const MAX_ERROR_MESSAGE_CHARS: usize = 2000;

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

    pub async fn history(&self, user_id: Uuid, q: &HistoryQuery) -> AppResult<HistoryPage> {
        let limit = q.limit.unwrap_or(50).clamp(1, 200);
        let search = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let mut items = repo::list_history(
            &self.state.db,
            user_id,
            &HistoryFilter {
                connection_id: q.connection_id,
                status: q.status,
                search,
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

    pub async fn create_collection(&self, user_id: Uuid, name: &str) -> AppResult<Collection> {
        repo::insert_collection(&self.state.db, user_id, name.trim())
            .await
            .map_err(reference_error)
    }

    pub async fn rename_collection(
        &self,
        user_id: Uuid,
        id: Uuid,
        name: &str,
    ) -> AppResult<Collection> {
        repo::rename_collection(&self.state.db, user_id, id, name.trim())
            .await
            .map_err(reference_error)?
            .ok_or(AppError::NotFound("collection"))
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
        Ok(repo::list_saved(&self.state.db, user_id, f.collection_id, search).await?)
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

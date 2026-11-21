//! Query performance history by fingerprint, and saved plans.

use uuid::Uuid;

use crate::{
    database::performance::{self as repo, NewPlan},
    errors::{AppError, AppResult},
    models::performance::{
        CreatePlanRequest, FingerprintStats, FingerprintTimeline, PerformanceQuery, SavedPlan,
        SavedPlanSummary, MAX_SAVED_PLANS,
    },
    state::AppState,
    utils::fingerprint::normalize,
};

/// Executions listed under a fingerprint's timeline.
const RECENT_EXECUTIONS: i64 = 20;
/// Normalised text is for display; very long statements are cut.
const MAX_NORMALIZED_CHARS: usize = 2000;

fn with_normalized(mut stats: FingerprintStats) -> FingerprintStats {
    stats.normalized_sql = normalize(&stats.sample_sql)
        .chars()
        .take(MAX_NORMALIZED_CHARS)
        .collect();
    stats
}

pub struct PerformanceService<'a> {
    state: &'a AppState,
}

impl<'a> PerformanceService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub async fn fingerprints(
        &self,
        user_id: Uuid,
        q: &PerformanceQuery,
    ) -> AppResult<Vec<FingerprintStats>> {
        let rows =
            repo::list_fingerprints(&self.state.db, user_id, q.days(), q.connection_id).await?;
        Ok(rows.into_iter().map(with_normalized).collect())
    }

    pub async fn timeline(
        &self,
        user_id: Uuid,
        fingerprint: i64,
        q: &PerformanceQuery,
    ) -> AppResult<FingerprintTimeline> {
        let db = &self.state.db;
        let stats = repo::fingerprint_stats(db, user_id, fingerprint, q.days(), q.connection_id)
            .await?
            .ok_or(AppError::NotFound("fingerprint"))?;
        let points =
            repo::fingerprint_timeline(db, user_id, fingerprint, q.days(), q.connection_id).await?;
        let recent = repo::recent_with_fingerprint(
            db,
            user_id,
            fingerprint,
            q.connection_id,
            RECENT_EXECUTIONS,
        )
        .await?;
        Ok(FingerprintTimeline {
            stats: with_normalized(stats),
            points,
            recent,
        })
    }

    // -- saved plans ----------------------------------------------------------

    pub async fn plans(&self, user_id: Uuid) -> AppResult<Vec<SavedPlanSummary>> {
        Ok(repo::list_plans(&self.state.db, user_id).await?)
    }

    pub async fn plan(&self, user_id: Uuid, id: Uuid) -> AppResult<SavedPlan> {
        repo::get_plan(&self.state.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("plan"))
    }

    pub async fn save_plan(&self, user_id: Uuid, req: &CreatePlanRequest) -> AppResult<SavedPlan> {
        let db = &self.state.db;
        if repo::count_plans(db, user_id).await? >= MAX_SAVED_PLANS {
            return Err(AppError::Conflict(format!(
                "at most {MAX_SAVED_PLANS} plans can be saved; delete some first"
            )));
        }
        let id = repo::insert_plan(
            db,
            &NewPlan {
                user_id,
                connection_id: req.connection_id,
                name: req.name.trim(),
                sql: &req.sql,
                plan: &req.plan,
                analyzed: req.analyzed(),
            },
        )
        .await
        .map_err(|e| match &e {
            // The connection belongs to someone else or no longer exists.
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                AppError::NotFound("connection")
            }
            _ => e.into(),
        })?;
        self.plan(user_id, id).await
    }

    pub async fn delete_plan(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_plan(&self.state.db, user_id, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("plan"))
        }
    }
}

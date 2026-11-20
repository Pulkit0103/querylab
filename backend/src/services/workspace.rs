//! The synced editor session and per-user workspace settings.

use uuid::Uuid;

use crate::{
    database::workspace as repo,
    errors::AppResult,
    models::workspace::{SessionResponse, WorkspaceSession, WorkspaceSettings},
    state::AppState,
};

pub struct WorkspaceService<'a> {
    state: &'a AppState,
}

impl<'a> WorkspaceService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub async fn session(&self, user_id: Uuid) -> AppResult<SessionResponse> {
        Ok(match repo::get_session(&self.state.db, user_id).await? {
            Some((session, updated_at)) => SessionResponse {
                session: Some(session),
                updated_at: Some(updated_at),
            },
            None => SessionResponse {
                session: None,
                updated_at: None,
            },
        })
    }

    /// Replaces the stored session (last write wins).
    pub async fn save_session(
        &self,
        user_id: Uuid,
        session: WorkspaceSession,
    ) -> AppResult<SessionResponse> {
        let updated_at = repo::put_session(&self.state.db, user_id, &session).await?;
        Ok(SessionResponse {
            session: Some(session),
            updated_at: Some(updated_at),
        })
    }

    pub async fn settings(&self, user_id: Uuid) -> AppResult<WorkspaceSettings> {
        Ok(repo::get_settings(&self.state.db, user_id).await?)
    }

    pub async fn save_settings(
        &self,
        user_id: Uuid,
        settings: &WorkspaceSettings,
    ) -> AppResult<WorkspaceSettings> {
        Ok(repo::put_settings(&self.state.db, user_id, settings).await?)
    }
}

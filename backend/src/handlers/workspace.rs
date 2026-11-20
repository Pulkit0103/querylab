//! Synced editor tabs and workspace settings.

use axum::{extract::State, Json};

use crate::{
    errors::AppResult,
    models::workspace::{SessionResponse, WorkspaceSession, WorkspaceSettings},
    services::{auth::AuthUser, workspace::WorkspaceService},
    state::AppState,
    utils::validation::ValidatedJson,
};

pub async fn get_tabs(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<SessionResponse>> {
    Ok(Json(
        WorkspaceService::new(&state).session(user.user_id).await?,
    ))
}

pub async fn put_tabs(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(session): ValidatedJson<WorkspaceSession>,
) -> AppResult<Json<SessionResponse>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .save_session(user.user_id, session)
            .await?,
    ))
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<WorkspaceSettings>> {
    Ok(Json(
        WorkspaceService::new(&state).settings(user.user_id).await?,
    ))
}

pub async fn put_settings(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(settings): ValidatedJson<WorkspaceSettings>,
) -> AppResult<Json<WorkspaceSettings>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .save_settings(user.user_id, &settings)
            .await?,
    ))
}

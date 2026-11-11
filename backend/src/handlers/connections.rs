use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::connection::{ConnectionResponse, CreateConnectionRequest, UpdateConnectionRequest},
    services::{auth::AuthUser, connections::ConnectionService},
    state::AppState,
    utils::validation::{ApiPath, ValidatedJson},
};

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<ConnectionResponse>>> {
    Ok(Json(
        ConnectionService::new(&state).list(user.user_id).await?,
    ))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ConnectionResponse>> {
    Ok(Json(
        ConnectionService::new(&state).get(user.user_id, id).await?,
    ))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateConnectionRequest>,
) -> AppResult<(StatusCode, Json<ConnectionResponse>)> {
    let created = ConnectionService::new(&state)
        .create(user.user_id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateConnectionRequest>,
) -> AppResult<Json<ConnectionResponse>> {
    Ok(Json(
        ConnectionService::new(&state)
            .update(user.user_id, id, &req)
            .await?,
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    ConnectionService::new(&state)
        .delete(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

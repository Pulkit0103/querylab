use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::connection::{
        ConnectionGroup, ConnectionGroupRequest, ConnectionResponse, CreateConnectionRequest,
        UpdateConnectionRequest,
    },
    services::{
        auth::AuthUser,
        connections::ConnectionService,
        target_db::{self, ConnectionTestResult, TargetSpec},
    },
    state::AppState,
    utils::{
        network,
        validation::{ApiPath, ValidatedJson},
    },
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

/// Tests an unsaved connection (used by the "Test connection" button before saving).
pub async fn test_draft(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateConnectionRequest>,
) -> AppResult<Json<ConnectionTestResult>> {
    state.connect_limiter.check(&user.user_id.to_string())?;
    network::precheck(
        req.host.trim(),
        req.port,
        state.config.allow_private_networks,
    )?;
    let result = target_db::test_connection(&state.config, &TargetSpec::from_request(&req)).await?;
    Ok(Json(result))
}

/// Tests a saved connection using its stored (encrypted) credentials.
pub async fn test_saved(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ConnectionTestResult>> {
    state.connect_limiter.check(&user.user_id.to_string())?;
    let result = ConnectionService::new(&state)
        .test_saved(user.user_id, id)
        .await?;
    Ok(Json(result))
}

/// Copies a connection (credentials included) under a new name.
pub async fn duplicate(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<(StatusCode, Json<ConnectionResponse>)> {
    let copy = ConnectionService::new(&state)
        .duplicate(user.user_id, id)
        .await?;
    Ok((StatusCode::CREATED, Json(copy)))
}

// -- groups ---------------------------------------------------------------

pub async fn list_groups(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<ConnectionGroup>>> {
    Ok(Json(
        ConnectionService::new(&state).groups(user.user_id).await?,
    ))
}

pub async fn create_group(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<ConnectionGroupRequest>,
) -> AppResult<(StatusCode, Json<ConnectionGroup>)> {
    let group = ConnectionService::new(&state)
        .create_group(user.user_id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(group)))
}

pub async fn update_group(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<ConnectionGroupRequest>,
) -> AppResult<Json<ConnectionGroup>> {
    Ok(Json(
        ConnectionService::new(&state)
            .update_group(user.user_id, id, &req)
            .await?,
    ))
}

pub async fn delete_group(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    ConnectionService::new(&state)
        .delete_group(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
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

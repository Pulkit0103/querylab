use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::connection::{ConnectionResponse, CreateConnectionRequest, UpdateConnectionRequest},
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
    let service = ConnectionService::new(&state);
    let record = service.get_record(user.user_id, id).await?;
    let secret = service.decrypt_secret(&record)?;
    let result =
        target_db::test_connection(&state.config, &TargetSpec::from_record(&record, &secret))
            .await?;
    service.mark_connected(user.user_id, id).await?;
    Ok(Json(result))
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

//! Live monitoring of a connection's server.

use axum::{extract::State, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::monitoring::{
        ActivityResponse, LocksResponse, Signal, SignalRequest, SignalResponse, SizesQuery,
        SizesResponse, TopQueriesQuery, TopQueriesResponse,
    },
    services::{auth::AuthUser, monitoring::MonitoringService},
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, Validate, ValidatedJson},
};

pub async fn activity(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<ActivityResponse>> {
    Ok(Json(
        MonitoringService::new(&state)
            .activity(user.user_id, id)
            .await?,
    ))
}

pub async fn locks(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<LocksResponse>> {
    Ok(Json(
        MonitoringService::new(&state)
            .locks(user.user_id, id)
            .await?,
    ))
}

pub async fn sizes(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<SizesQuery>,
) -> AppResult<Json<SizesResponse>> {
    query.validate()?;
    Ok(Json(
        MonitoringService::new(&state)
            .sizes(user.user_id, id, query.schema.as_deref())
            .await?,
    ))
}

pub async fn top_queries(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<TopQueriesQuery>,
) -> AppResult<Json<TopQueriesResponse>> {
    Ok(Json(
        MonitoringService::new(&state)
            .top_queries(user.user_id, id, query.sort)
            .await?,
    ))
}

async fn signal(
    state: AppState,
    user: AuthUser,
    id: Uuid,
    pid: i32,
    signal: Signal,
    req: SignalRequest,
) -> AppResult<Json<SignalResponse>> {
    Ok(Json(
        MonitoringService::new(&state)
            .signal(user.user_id, id, pid, signal, req.confirm.as_deref())
            .await?,
    ))
}

pub async fn cancel(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, pid)): ApiPath<(Uuid, i32)>,
    ValidatedJson(req): ValidatedJson<SignalRequest>,
) -> AppResult<Json<SignalResponse>> {
    signal(state, user, id, pid, Signal::Cancel, req).await
}

pub async fn terminate(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, pid)): ApiPath<(Uuid, i32)>,
    ValidatedJson(req): ValidatedJson<SignalRequest>,
) -> AppResult<Json<SignalResponse>> {
    signal(state, user, id, pid, Signal::Terminate, req).await
}

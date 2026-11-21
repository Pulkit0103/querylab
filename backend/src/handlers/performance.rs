//! Query performance history and saved plans.

use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::performance::{
        CreatePlanRequest, FingerprintStats, FingerprintTimeline, PerformanceQuery, SavedPlan,
        SavedPlanSummary,
    },
    services::{auth::AuthUser, performance::PerformanceService},
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, Validate, ValidatedJson},
};

pub async fn fingerprints(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(query): ApiQuery<PerformanceQuery>,
) -> AppResult<Json<Vec<FingerprintStats>>> {
    query.validate()?;
    Ok(Json(
        PerformanceService::new(&state)
            .fingerprints(user.user_id, &query)
            .await?,
    ))
}

pub async fn timeline(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(fingerprint): ApiPath<String>,
    ApiQuery(query): ApiQuery<PerformanceQuery>,
) -> AppResult<Json<FingerprintTimeline>> {
    query.validate()?;
    let fingerprint: i64 = fingerprint
        .parse()
        .map_err(|_| AppError::validation("fingerprint", "must be a 64-bit integer"))?;
    Ok(Json(
        PerformanceService::new(&state)
            .timeline(user.user_id, fingerprint, &query)
            .await?,
    ))
}

pub async fn list_plans(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<SavedPlanSummary>>> {
    Ok(Json(
        PerformanceService::new(&state).plans(user.user_id).await?,
    ))
}

pub async fn get_plan(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<SavedPlan>> {
    Ok(Json(
        PerformanceService::new(&state)
            .plan(user.user_id, id)
            .await?,
    ))
}

pub async fn create_plan(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreatePlanRequest>,
) -> AppResult<(StatusCode, Json<SavedPlan>)> {
    let plan = PerformanceService::new(&state)
        .save_plan(user.user_id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(plan)))
}

pub async fn delete_plan(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    PerformanceService::new(&state)
        .delete_plan(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

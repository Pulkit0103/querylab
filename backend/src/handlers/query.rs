use axum::{extract::State, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::query::{CancelResponse, ExecuteRequest, ExplainRequest, ExplainResult, QueryResult},
    services::{auth::AuthUser, execution, library::LibraryService},
    state::AppState,
    utils::validation::{ApiPath, ValidatedJson},
};

pub async fn execute(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<ExecuteRequest>,
) -> AppResult<Json<QueryResult>> {
    let outcome = execution::execute(&state, &user, &req).await;
    let history_id = LibraryService::new(&state)
        .record_execution(user.user_id, req.connection_id, &req.query, &outcome)
        .await;
    let mut result = outcome?;
    result.history_id = history_id;
    Ok(Json(result))
}

pub async fn explain(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<ExplainRequest>,
) -> AppResult<Json<ExplainResult>> {
    Ok(Json(execution::explain(&state, &user, &req).await?))
}

pub async fn cancel(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(execution_id): ApiPath<Uuid>,
) -> AppResult<Json<CancelResponse>> {
    let cancelled = execution::cancel(&state, &user, execution_id).await?;
    Ok(Json(CancelResponse { cancelled }))
}

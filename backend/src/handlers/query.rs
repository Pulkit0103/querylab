use axum::{extract::State, Json};
use serde_json::json;
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::query::{CancelResponse, ExecuteRequest, ExplainRequest, ExplainResult, QueryResult},
    services::{
        audit::{self, AuditEvent},
        auth::AuthUser,
        execution,
        library::LibraryService,
    },
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
    let detail = match &outcome {
        Ok(r) => json!({
            "status": "success",
            "rows": r.row_count,
            "rows_affected": r.rows_affected,
            "duration_ms": r.execution_time_ms,
        }),
        Err(e) => json!({ "status": e.code() }),
    };
    audit::record(
        &state.db,
        AuditEvent::new(Some(user.user_id), "query.executed")
            .connection(req.connection_id)
            .detail(detail)
            .sql(&req.query),
    )
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
    let outcome = execution::explain(&state, &user, &req).await;
    // ANALYZE executes the statement, so it is audited like an execution.
    if req.analyze {
        audit::record(
            &state.db,
            AuditEvent::new(Some(user.user_id), "query.explain_analyze")
                .connection(req.connection_id)
                .detail(
                    json!({ "status": outcome.as_ref().map_or_else(|e| e.code(), |_| "success") }),
                )
                .sql(&req.query),
        )
        .await;
    }
    Ok(Json(outcome?))
}

pub async fn cancel(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(execution_id): ApiPath<Uuid>,
) -> AppResult<Json<CancelResponse>> {
    let cancelled = execution::cancel(&state, &user, execution_id).await?;
    Ok(Json(CancelResponse { cancelled }))
}

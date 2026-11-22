//! The audit log: the user's own events (and, with workspaces, the events
//! of workspaces they administer).

use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};

use crate::{
    errors::AppResult,
    models::audit::{AuditPage, AuditQuery},
    services::{audit_log::AuditLogService, auth::AuthUser},
    state::AppState,
    utils::validation::ApiQuery,
};

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> AppResult<Json<AuditPage>> {
    Ok(Json(AuditLogService::new(&state).page(&user, &q).await?))
}

pub async fn export(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> AppResult<Response> {
    let csv = AuditLogService::new(&state).csv(&user, &q).await?;
    let mut res = csv.into_response();
    let headers = res.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"audit-log.csv\""),
    );
    Ok(res)
}

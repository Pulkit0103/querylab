//! Data Studio endpoints: browse, preview, apply and export table rows.

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::data::{
        ApplyRequest, ApplyResponse, BrowseRequest, BrowseResponse, ExportRequest, PreviewRequest,
        PreviewResponse,
    },
    services::{auth::AuthUser, data_studio::DataStudioService},
    state::AppState,
    utils::validation::{ApiPath, ValidatedJson},
};

pub async fn browse(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<BrowseRequest>,
) -> AppResult<Json<BrowseResponse>> {
    Ok(Json(
        DataStudioService::new(&state)
            .browse(&user, id, &req)
            .await?,
    ))
}

pub async fn preview(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<PreviewRequest>,
) -> AppResult<Json<PreviewResponse>> {
    Ok(Json(
        DataStudioService::new(&state)
            .preview(&user, id, &req)
            .await?,
    ))
}

pub async fn apply(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<ApplyRequest>,
) -> AppResult<Json<ApplyResponse>> {
    Ok(Json(
        DataStudioService::new(&state)
            .apply(&user, id, &req)
            .await?,
    ))
}

pub async fn export(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<ExportRequest>,
) -> AppResult<Response> {
    let export = DataStudioService::new(&state)
        .export(&user, id, &req)
        .await?;
    let stream = futures_util::stream::unfold(export.body, |mut rx| async move {
        rx.recv().await.map(|chunk| (chunk, rx))
    });
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(export.content_type),
    );
    if let Ok(value) =
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", export.filename))
    {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

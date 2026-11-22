//! Imports, exports, mock data and the data job log.

use axum::{body::Bytes, extract::State, http::StatusCode, response::Response, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    handlers::data::export_response,
    models::data_jobs::{
        CheckImportRequest, CheckImportResponse, CreateImportRequest, CreateImportResponse,
        ExportSource, ExportsRequest, ImportPreview, JobKind, JobRecord, JobView, JobsQuery,
        MockPreview, MockRequest, ParseOptions, RunImportRequest, UploadQuery,
    },
    services::{
        auth::AuthUser, data_jobs, export::ExportService, import::ImportService,
        mock_data::MockDataService,
    },
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, ValidatedJson},
};

pub async fn create_import(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(connection_id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<CreateImportRequest>,
) -> AppResult<(StatusCode, Json<CreateImportResponse>)> {
    let created = ImportService::new(&state)
        .create(user.user_id, connection_id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn upload(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<UploadQuery>,
    body: Bytes,
) -> AppResult<Json<JobView>> {
    Ok(Json(ImportService::new(&state).upload(
        user.user_id,
        id,
        query.offset,
        body,
    )?))
}

pub async fn analyze(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(opts): ValidatedJson<ParseOptions>,
) -> AppResult<Json<ImportPreview>> {
    Ok(Json(
        ImportService::new(&state)
            .analyze(user.user_id, id, opts)
            .await?,
    ))
}

pub async fn check(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<CheckImportRequest>,
) -> AppResult<Json<CheckImportResponse>> {
    Ok(Json(
        ImportService::new(&state)
            .check(user.user_id, id, &req)
            .await?,
    ))
}

pub async fn run_import(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<RunImportRequest>,
) -> AppResult<(StatusCode, Json<JobView>)> {
    let view = ImportService::new(&state).run(&user, id, &req).await?;
    Ok((StatusCode::ACCEPTED, Json(view)))
}

pub async fn import_status(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<JobView>> {
    Ok(Json(
        data_jobs::view(&state, user.user_id, id, Some(JobKind::Import)).await?,
    ))
}

pub async fn delete_import(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    data_jobs::cancel(&state, user.user_id, id)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn export(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(connection_id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<ExportsRequest>,
) -> AppResult<Response> {
    let service = ExportService::new(&state);
    let stream = match &req.source {
        ExportSource::Table(source) => {
            service
                .table(&user, connection_id, source, req.format)
                .await?
        }
        ExportSource::Query { sql, name } => {
            service
                .query(&user, connection_id, sql, name.as_deref(), req.format)
                .await?
        }
    };
    Ok(export_response(stream))
}

pub async fn mock_preview(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(connection_id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<MockRequest>,
) -> AppResult<Json<MockPreview>> {
    Ok(Json(
        MockDataService::new(&state)
            .preview(user.user_id, connection_id, &req)
            .await?,
    ))
}

pub async fn mock_run(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(connection_id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<MockRequest>,
) -> AppResult<(StatusCode, Json<JobView>)> {
    let view = MockDataService::new(&state)
        .run(&user, connection_id, &req)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(view)))
}

pub async fn list_jobs(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(query): ApiQuery<JobsQuery>,
) -> AppResult<Json<Vec<JobRecord>>> {
    Ok(Json(data_jobs::list(&state, user.user_id, &query).await?))
}

pub async fn get_job(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<JobView>> {
    Ok(Json(data_jobs::view(&state, user.user_id, id, None).await?))
}

pub async fn cancel_job(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    data_jobs::cancel(&state, user.user_id, id)?;
    Ok(StatusCode::ACCEPTED)
}

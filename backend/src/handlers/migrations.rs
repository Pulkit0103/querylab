//! Schema comparison and migration management.

use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::{
        migrations::{
            ApplyRequest, CreateMigrationRequest, CreateProjectRequest, Migration, MigrationRun,
            Project, RunOutcome, RunsQuery, StatusQuery, StatusResponse, UpdateMigrationRequest,
        },
        schema_diff::{DiffRequest, DiffResponse},
    },
    services::{auth::AuthUser, migrations::MigrationService, schema::SchemaService},
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, ValidatedJson},
};

pub async fn schema_diff(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<DiffRequest>,
) -> AppResult<Json<DiffResponse>> {
    Ok(Json(
        SchemaService::new(&state).diff(user.user_id, &req).await?,
    ))
}

pub async fn list_projects(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Project>>> {
    Ok(Json(
        MigrationService::new(&state).projects(user.user_id).await?,
    ))
}

pub async fn create_project(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateProjectRequest>,
) -> AppResult<(StatusCode, Json<Project>)> {
    let project = MigrationService::new(&state)
        .create_project(user.user_id, &req.name)
        .await?;
    Ok((StatusCode::CREATED, Json(project)))
}

pub async fn delete_project(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    MigrationService::new(&state)
        .delete_project(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_migrations(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Vec<Migration>>> {
    Ok(Json(
        MigrationService::new(&state)
            .migrations(user.user_id, id)
            .await?,
    ))
}

pub async fn create_migration(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<CreateMigrationRequest>,
) -> AppResult<(StatusCode, Json<Migration>)> {
    let migration = MigrationService::new(&state)
        .create_migration(user.user_id, id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(migration)))
}

pub async fn get_migration(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Migration>> {
    Ok(Json(
        MigrationService::new(&state)
            .migration(user.user_id, id)
            .await?,
    ))
}

pub async fn update_migration(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateMigrationRequest>,
) -> AppResult<Json<Migration>> {
    Ok(Json(
        MigrationService::new(&state)
            .update_migration(user.user_id, id, &req)
            .await?,
    ))
}

pub async fn delete_migration(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    MigrationService::new(&state)
        .delete_migration(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn status(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<StatusQuery>,
) -> AppResult<Json<StatusResponse>> {
    Ok(Json(
        MigrationService::new(&state)
            .status(user.user_id, id, query.connection_id)
            .await?,
    ))
}

pub async fn apply(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<ApplyRequest>,
) -> AppResult<Json<RunOutcome>> {
    Ok(Json(
        MigrationService::new(&state)
            .apply(
                user.user_id,
                id,
                req.connection_id,
                req.confirm.as_deref(),
                req.dry_run,
            )
            .await?,
    ))
}

pub async fn rollback(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<ApplyRequest>,
) -> AppResult<Json<RunOutcome>> {
    Ok(Json(
        MigrationService::new(&state)
            .rollback(
                user.user_id,
                id,
                req.connection_id,
                req.confirm.as_deref(),
                req.dry_run,
            )
            .await?,
    ))
}

pub async fn runs(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(query): ApiQuery<RunsQuery>,
) -> AppResult<Json<Vec<MigrationRun>>> {
    Ok(Json(
        MigrationService::new(&state)
            .runs(user.user_id, query.project_id, query.connection_id)
            .await?,
    ))
}

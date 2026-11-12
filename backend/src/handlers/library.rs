use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::library::{
        Collection, CollectionRequest, CreateSavedQuery, HistoryEntry, HistoryPage, HistoryQuery,
        SavedQuery, SavedQueryFilter, StatsQuery, StatsResponse, UpdateSavedQuery,
    },
    services::{auth::AuthUser, library::LibraryService},
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, ValidatedJson},
};

// -- history ------------------------------------------------------------------

pub async fn list_history(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(q): ApiQuery<HistoryQuery>,
) -> AppResult<Json<HistoryPage>> {
    Ok(Json(
        LibraryService::new(&state)
            .history(user.user_id, &q)
            .await?,
    ))
}

pub async fn get_history(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<HistoryEntry>> {
    Ok(Json(
        LibraryService::new(&state)
            .history_entry(user.user_id, id)
            .await?,
    ))
}

pub async fn delete_history(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    LibraryService::new(&state)
        .delete_history(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct Cleared {
    pub deleted: u64,
}

pub async fn clear_history(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Cleared>> {
    let deleted = LibraryService::new(&state)
        .clear_history(user.user_id)
        .await?;
    Ok(Json(Cleared { deleted }))
}

// -- collections --------------------------------------------------------------

pub async fn list_collections(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Collection>>> {
    Ok(Json(
        LibraryService::new(&state)
            .collections(user.user_id)
            .await?,
    ))
}

pub async fn create_collection(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CollectionRequest>,
) -> AppResult<(StatusCode, Json<Collection>)> {
    let c = LibraryService::new(&state)
        .create_collection(user.user_id, &req.name)
        .await?;
    Ok((StatusCode::CREATED, Json(c)))
}

pub async fn rename_collection(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<CollectionRequest>,
) -> AppResult<Json<Collection>> {
    Ok(Json(
        LibraryService::new(&state)
            .rename_collection(user.user_id, id, &req.name)
            .await?,
    ))
}

pub async fn delete_collection(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    LibraryService::new(&state)
        .delete_collection(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// -- saved queries ------------------------------------------------------------

pub async fn list_saved(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(f): ApiQuery<SavedQueryFilter>,
) -> AppResult<Json<Vec<SavedQuery>>> {
    Ok(Json(
        LibraryService::new(&state)
            .saved_queries(user.user_id, &f)
            .await?,
    ))
}

pub async fn get_saved(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<SavedQuery>> {
    Ok(Json(
        LibraryService::new(&state)
            .saved_query(user.user_id, id)
            .await?,
    ))
}

pub async fn create_saved(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<CreateSavedQuery>,
) -> AppResult<(StatusCode, Json<SavedQuery>)> {
    let q = LibraryService::new(&state)
        .create_saved(user.user_id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(q)))
}

pub async fn update_saved(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateSavedQuery>,
) -> AppResult<Json<SavedQuery>> {
    Ok(Json(
        LibraryService::new(&state)
            .update_saved(user.user_id, id, &req)
            .await?,
    ))
}

pub async fn delete_saved(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    LibraryService::new(&state)
        .delete_saved(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// -- stats --------------------------------------------------------------------

pub async fn stats(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> AppResult<Json<StatsResponse>> {
    Ok(Json(
        LibraryService::new(&state)
            .stats(user.user_id, q.days)
            .await?,
    ))
}

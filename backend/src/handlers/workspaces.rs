//! Team workspaces, members, invitations and snippets.

use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::workspaces::{
        CreatedInvitation, Invitation, InvitationPreview, InviteRequest, Member, RoleRequest,
        Snippet, SnippetRequest, WorkspaceRequest, WorkspaceScope, WorkspaceSummary,
    },
    services::{auth::AuthUser, workspaces::WorkspaceService},
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, ValidatedJson},
};

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<WorkspaceSummary>>> {
    Ok(Json(
        WorkspaceService::new(&state).list(user.user_id).await?,
    ))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<WorkspaceRequest>,
) -> AppResult<(StatusCode, Json<WorkspaceSummary>)> {
    let ws = WorkspaceService::new(&state)
        .create(user.user_id, &req.name)
        .await?;
    Ok((StatusCode::CREATED, Json(ws)))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<WorkspaceSummary>> {
    Ok(Json(
        WorkspaceService::new(&state).get(user.user_id, id).await?,
    ))
}

pub async fn rename(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<WorkspaceRequest>,
) -> AppResult<Json<WorkspaceSummary>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .rename(user.user_id, id, &req.name)
            .await?,
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    WorkspaceService::new(&state)
        .delete(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn members(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Vec<Member>>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .members(user.user_id, id)
            .await?,
    ))
}

pub async fn set_role(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, member)): ApiPath<(Uuid, Uuid)>,
    ValidatedJson(req): ValidatedJson<RoleRequest>,
) -> AppResult<Json<Vec<Member>>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .set_role(user.user_id, id, member, req.role)
            .await?,
    ))
}

pub async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, member)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    WorkspaceService::new(&state)
        .remove_member(user.user_id, id, member)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn invite(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<InviteRequest>,
) -> AppResult<(StatusCode, Json<CreatedInvitation>)> {
    let created = WorkspaceService::new(&state)
        .invite(&user, id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn invitations(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Vec<Invitation>>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .invitations(user.user_id, id)
            .await?,
    ))
}

pub async fn revoke_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, invitation)): ApiPath<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    WorkspaceService::new(&state)
        .revoke_invitation(user.user_id, id, invitation)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn preview_invitation(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(token): ApiPath<String>,
) -> AppResult<Json<InvitationPreview>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .preview_invitation(&token)
            .await?,
    ))
}

pub async fn accept_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(token): ApiPath<String>,
) -> AppResult<Json<WorkspaceSummary>> {
    state
        .auth_limiter
        .check(&format!("invite:{}", user.user_id))?;
    Ok(Json(
        WorkspaceService::new(&state).accept(&user, &token).await?,
    ))
}

// -- snippets -------------------------------------------------------------

pub async fn list_snippets(
    State(state): State<AppState>,
    user: AuthUser,
    ApiQuery(scope): ApiQuery<WorkspaceScope>,
) -> AppResult<Json<Vec<Snippet>>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .snippets(user.user_id, scope.workspace_id)
            .await?,
    ))
}

pub async fn create_snippet(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<SnippetRequest>,
) -> AppResult<(StatusCode, Json<Snippet>)> {
    let s = WorkspaceService::new(&state)
        .create_snippet(user.user_id, &req)
        .await?;
    Ok((StatusCode::CREATED, Json(s)))
}

pub async fn get_snippet(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Snippet>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .snippet(user.user_id, id)
            .await?,
    ))
}

pub async fn update_snippet(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(req): ValidatedJson<SnippetRequest>,
) -> AppResult<Json<Snippet>> {
    Ok(Json(
        WorkspaceService::new(&state)
            .update_snippet(user.user_id, id, &req)
            .await?,
    ))
}

pub async fn delete_snippet(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    WorkspaceService::new(&state)
        .delete_snippet(user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

//! Account security endpoints: password, sessions, two-factor
//! authentication and password reset.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    database::account::SessionInfo,
    errors::AppResult,
    handlers::auth::no_store,
    middleware::client_ip::ClientIp,
    models::auth::{
        ChangePasswordRequest, DisableTotpRequest, ResetCheckQuery, ResetConfirmRequest,
        ResetRequest, TotpCodeRequest,
    },
    services::{
        account::{AccountService, RecoveryCodes, SecurityStatus, TotpEnrollment},
        auth::AuthUser,
    },
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, ValidatedJson},
};

pub async fn security(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<SecurityStatus>> {
    Ok(Json(
        AccountService::new(&state).status(user.user_id).await?,
    ))
}

pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<ChangePasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    state
        .auth_limiter
        .check(&format!("password:{}", user.user_id))?;
    let revoked = AccountService::new(&state)
        .change_password(&user, req.current_password, req.new_password)
        .await?;
    Ok(Json(json!({ "other_sessions_revoked": revoked })))
}

pub async fn sessions(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<SessionInfo>>> {
    Ok(Json(AccountService::new(&state).sessions(&user).await?))
}

pub async fn revoke_session(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<StatusCode> {
    AccountService::new(&state)
        .revoke_session(&user, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Signs out every session except the current one.
pub async fn revoke_other_sessions(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    let revoked = AccountService::new(&state)
        .revoke_other_sessions(&user)
        .await?;
    Ok(Json(json!({ "revoked": revoked })))
}

pub async fn totp_enroll(State(state): State<AppState>, user: AuthUser) -> AppResult<Response> {
    let enrollment: TotpEnrollment = AccountService::new(&state).totp_enroll(&user).await?;
    Ok(no_store(Json(enrollment).into_response()))
}

pub async fn totp_verify(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<TotpCodeRequest>,
) -> AppResult<Response> {
    state
        .auth_limiter
        .check(&format!("totp:{}", user.user_id))?;
    let codes: RecoveryCodes = AccountService::new(&state)
        .totp_confirm(&user, &req.code)
        .await?;
    Ok(no_store(Json(codes).into_response()))
}

pub async fn recovery_codes(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<TotpCodeRequest>,
) -> AppResult<Response> {
    state
        .auth_limiter
        .check(&format!("totp:{}", user.user_id))?;
    let codes = AccountService::new(&state)
        .regenerate_recovery_codes(&user, &req.code)
        .await?;
    Ok(no_store(Json(codes).into_response()))
}

pub async fn totp_disable(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<DisableTotpRequest>,
) -> AppResult<StatusCode> {
    state
        .auth_limiter
        .check(&format!("totp:{}", user.user_id))?;
    AccountService::new(&state)
        .totp_disable(&user, req.password, &req.code)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Always 202: the response does not reveal whether the account exists.
pub async fn request_reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ValidatedJson(req): ValidatedJson<ResetRequest>,
) -> AppResult<StatusCode> {
    state.auth_limiter.check(&format!("reset-ip:{ip}"))?;
    AccountService::new(&state)
        .request_reset(&req.email)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn check_reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiQuery(q): ApiQuery<ResetCheckQuery>,
) -> AppResult<Json<serde_json::Value>> {
    state.auth_limiter.check(&format!("reset-ip:{ip}"))?;
    let valid = AccountService::new(&state)
        .reset_token_is_valid(&q.token)
        .await?;
    Ok(Json(json!({ "valid": valid })))
}

pub async fn confirm_reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ValidatedJson(req): ValidatedJson<ResetConfirmRequest>,
) -> AppResult<StatusCode> {
    state.auth_limiter.check(&format!("reset-ip:{ip}"))?;
    AccountService::new(&state)
        .confirm_reset(&req.token, req.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

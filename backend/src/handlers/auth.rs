use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::{
    errors::AppResult,
    middleware::{
        auth::{clear_session_cookie, session_cookie},
        client_ip::ClientIp,
    },
    models::{
        auth::{LoginRequest, RegisterRequest, TwoFactorLoginRequest},
        user::UserResponse,
    },
    services::auth::{AuthService, AuthUser, IssuedSession, LoginOutcome},
    state::AppState,
    utils::validation::ValidatedJson,
};

pub(crate) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
}

pub(crate) fn no_store(mut res: Response) -> Response {
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    res
}

fn with_session(
    status: StatusCode,
    user: UserResponse,
    session: &IssuedSession,
    secure: bool,
) -> Response {
    let mut res = (status, Json(user)).into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&session.token, session.expires_at, secure),
    );
    // Responses that set credentials must never be cached.
    no_store(res)
}

pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<RegisterRequest>,
) -> AppResult<Response> {
    state.auth_limiter.check(&format!("register:{ip}"))?;
    let (user, session) = AuthService::new(&state)
        .register(req, user_agent(&headers))
        .await?;
    Ok(with_session(
        StatusCode::CREATED,
        user,
        &session,
        state.config.cookie_secure,
    ))
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> AppResult<Response> {
    // Limit per IP and per account to slow down both spraying and targeted guessing.
    state.auth_limiter.check(&format!("login-ip:{ip}"))?;
    state
        .auth_limiter
        .check(&format!("login-user:{}", req.email.trim().to_lowercase()))?;
    match AuthService::new(&state)
        .login(req, user_agent(&headers))
        .await?
    {
        LoginOutcome::Session(user, session) => Ok(with_session(
            StatusCode::OK,
            user,
            &session,
            state.config.cookie_secure,
        )),
        // No session yet: the client asks for a code and finishes with
        // `POST /auth/login/2fa`.
        LoginOutcome::TwoFactor { challenge } => Ok(no_store(
            Json(json!({ "two_factor_required": true, "challenge": challenge.as_str() }))
                .into_response(),
        )),
    }
}

pub async fn login_two_factor(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<TwoFactorLoginRequest>,
) -> AppResult<Response> {
    state.auth_limiter.check(&format!("login-2fa:{ip}"))?;
    let (user, session) = AuthService::new(&state)
        .login_second_factor(&req.challenge, &req.code, user_agent(&headers))
        .await?;
    Ok(with_session(
        StatusCode::OK,
        user,
        &session,
        state.config.cookie_secure,
    ))
}

pub async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<Response> {
    AuthService::new(&state).logout(&user).await?;
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        clear_session_cookie(state.config.cookie_secure),
    );
    Ok(res)
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<UserResponse>> {
    let record = crate::database::users::find_by_id(&state.db, user.user_id)
        .await?
        .ok_or(crate::errors::AppError::Unauthorized)?;
    Ok(Json(UserResponse::from(&record)))
}

/// Public: which optional account features this server offers.
pub async fn options(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "password_reset": state.mailer.is_enabled() }))
}

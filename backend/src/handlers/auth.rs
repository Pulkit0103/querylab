use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

use crate::{
    errors::AppResult,
    middleware::{
        auth::{clear_session_cookie, session_cookie},
        client_ip::ClientIp,
    },
    models::{
        auth::{LoginRequest, RegisterRequest},
        user::UserResponse,
    },
    services::auth::{AuthService, AuthUser, IssuedSession},
    state::AppState,
    utils::validation::ValidatedJson,
};

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
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
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    res
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
    let (user, session) = AuthService::new(&state)
        .login(req, user_agent(&headers))
        .await?;
    Ok(with_session(
        StatusCode::OK,
        user,
        &session,
        state.config.cookie_secure,
    ))
}

pub async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<Response> {
    AuthService::new(&state).logout(user.session_id).await?;
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

#![allow(dead_code)]

use std::time::Duration;

use axum::{body::Body, http::Request, Router};
use http_body_util::BodyExt;
use querylab_api::{build_router, config::Config, AppState};
use sqlx::{postgres::PgPoolOptions, PgPool};

/// A config pointing at an unreachable database. Useful for exercising HTTP
/// behaviour that must not depend on the database (validation, auth guards).
pub fn offline_config() -> Config {
    Config::for_tests("postgres://nobody:nothing@127.0.0.1:1/none")
}

pub fn offline_state() -> AppState {
    let config = offline_config();
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy(&config.database_url)
        .unwrap();
    AppState::new(config, pool)
}

pub fn offline_app() -> Router {
    build_router(offline_state())
}

/// App backed by a migrated test database (from `#[sqlx::test]`).
pub fn app_with_pool(pool: PgPool) -> (Router, AppState) {
    let state = AppState::new(Config::for_tests("postgres://unused"), pool);
    (build_router(state.clone()), state)
}

/// App backed by a test database with a custom config tweak.
pub fn app_with_config(pool: PgPool, tweak: impl FnOnce(&mut Config)) -> (Router, AppState) {
    let mut config = Config::for_tests("postgres://unused");
    tweak(&mut config);
    let state = AppState::new(config, pool);
    (build_router(state.clone()), state)
}

/// Connection payload for the seeded sample database (see
/// `scripts/sample-db`). Override with SAMPLE_PG_HOST / SAMPLE_PG_PORT.
pub fn sample_connection(name: &str) -> serde_json::Value {
    let host = std::env::var("SAMPLE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("SAMPLE_PG_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(5432);
    serde_json::json!({
        "name": name,
        "host": host,
        "port": port,
        "database": "sampledb",
        "username": "querylab_reader",
        "password": "reader_password",
        "ssl_mode": "disable",
        "read_only": true,
    })
}

/// Builds a JSON API request with the CSRF header and optional bearer token.
pub fn api(
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    token: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-querylab-csrf", "1");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

/// Registers a user and returns its session token (from the Set-Cookie header).
pub async fn register(app: &Router, email: &str) -> String {
    let (status, headers, body) = send_full(
        app,
        api(
            "POST",
            "/api/auth/register",
            Some(serde_json::json!({ "email": email, "password": "correct horse battery" })),
            None,
        ),
    )
    .await;
    assert_eq!(status, 201, "register failed: {body}");
    session_token(&headers).expect("session cookie")
}

pub fn session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = headers.get("set-cookie")?.to_str().ok()?;
    let (_, rest) = cookie.split_once('=')?;
    Some(rest.split(';').next()?.to_string())
}

pub async fn send(app: &Router, req: Request<Body>) -> (axum::http::StatusCode, serde_json::Value) {
    let (status, _, json) = send_full(app, req).await;
    (status, json)
}

pub async fn send_full(
    app: &Router,
    req: Request<Body>,
) -> (
    axum::http::StatusCode,
    axum::http::HeaderMap,
    serde_json::Value,
) {
    use tower::ServiceExt;
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, headers, json)
}

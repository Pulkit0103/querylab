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

pub async fn send(app: &Router, req: Request<Body>) -> (axum::http::StatusCode, serde_json::Value) {
    let (status, _, json) = send_full(app, req).await;
    (status, json)
}

pub async fn send_full(
    app: &Router,
    req: Request<Body>,
) -> (axum::http::StatusCode, axum::http::HeaderMap, serde_json::Value) {
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

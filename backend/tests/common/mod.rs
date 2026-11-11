#![allow(dead_code)]

use std::time::Duration;

use axum::{body::Body, http::Request, Router};
use http_body_util::BodyExt;
use querylab_api::{
    build_router,
    config::{Config, Environment, LogFormat},
    AppState,
};
use sqlx::postgres::PgPoolOptions;

/// A config pointing at an unreachable database. Useful for exercising HTTP
/// behaviour that must not depend on the database (validation, auth guards).
pub fn offline_config() -> Config {
    Config {
        environment: Environment::Development,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        database_url: "postgres://nobody:nothing@127.0.0.1:1/none".into(),
        database_max_connections: 1,
        cors_origins: vec!["http://localhost:3000".into()],
        log_format: LogFormat::Pretty,
        request_timeout: Duration::from_secs(10),
    }
}

pub fn offline_app() -> Router {
    let config = offline_config();
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy(&config.database_url)
        .unwrap();
    build_router(AppState::new(config, pool))
}

pub async fn send(app: &Router, req: Request<Body>) -> (axum::http::StatusCode, serde_json::Value) {
    use tower::ServiceExt;
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

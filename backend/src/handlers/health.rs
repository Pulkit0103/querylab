use std::time::Duration;

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{database, state::AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub database: &'static str,
    /// Redis for shared limits: `ok`, `unavailable` (per-instance fallback)
    /// or `disabled` (not configured).
    pub shared_state: &'static str,
}

/// Liveness + readiness probe. Returns 503 when the metadata database is not
/// reachable so load balancers stop routing traffic to this instance. A
/// Redis outage is reported but does not fail the probe: the instance keeps
/// serving with per-instance limits.
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let db_ok = database::ping(&state.db).await;
    let shared_state = match state.executions.shared() {
        None => "disabled",
        Some(store) => match tokio::time::timeout(Duration::from_secs(1), store.ping()).await {
            Ok(Ok(())) => "ok",
            _ => "unavailable",
        },
    };
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(HealthResponse {
            status: if db_ok { "ok" } else { "degraded" },
            version: env!("CARGO_PKG_VERSION"),
            database: if db_ok { "ok" } else { "unavailable" },
            shared_state,
        }),
    )
}

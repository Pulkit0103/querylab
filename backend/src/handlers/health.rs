use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{database, state::AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub database: &'static str,
}

/// Liveness + readiness probe. Returns 503 when the metadata database is not
/// reachable so load balancers stop routing traffic to this instance.
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let db_ok = database::ping(&state.db).await;
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
        }),
    )
}

//! Route table and cross-cutting HTTP middleware.

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderName, HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{handlers, state::AppState};

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Maximum accepted request body (SQL text is capped at 1 MiB separately).
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/version", get(handlers::version::version));

    Router::new()
        .route("/health", get(handlers::health::health))
        .nest("/api", api)
        .fallback(handlers::not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            state.config.request_timeout,
        ))
        .layer(cors_layer(&state.config.cors_origins))
        .layer(PropagateRequestIdLayer::new(REQUEST_ID))
        .layer(TraceLayer::new_for_http().make_span_with(
            |req: &axum::http::Request<axum::body::Body>| {
                let request_id = req
                    .headers()
                    .get(&REQUEST_ID)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("-");
                // Only method + path are logged; query strings and bodies may
                // contain sensitive data and are intentionally omitted.
                tracing::info_span!(
                    "http",
                    method = %req.method(),
                    path = %req.uri().path(),
                    request_id = %request_id,
                )
            },
        ))
        .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
        .with_state(state)
}

fn cors_layer(origins: &[String]) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true)
}

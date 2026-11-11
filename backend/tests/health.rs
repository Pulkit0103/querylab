mod common;

use axum::{body::Body, http::Request};

#[tokio::test]
async fn health_reports_degraded_when_database_unreachable() {
    let app = common::offline_app();
    for path in ["/health", "/api/health"] {
        let (status, body) =
            common::send(&app, Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(status, 503, "{path}");
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["database"], "unavailable");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = common::offline_app();
    let (status, _) = common::send(
        &app,
        Request::get("/does-not-exist").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, 404);
}

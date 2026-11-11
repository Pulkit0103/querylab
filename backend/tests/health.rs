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
async fn unknown_route_returns_json_404() {
    let app = common::offline_app();
    let (status, body) = common::send(
        &app,
        Request::get("/does-not-exist").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn version_endpoint() {
    let app = common::offline_app();
    let (status, body) =
        common::send(&app, Request::get("/api/version").body(Body::empty()).unwrap()).await;
    assert_eq!(status, 200);
    assert_eq!(body["name"], "querylab-api");
    assert_eq!(body["api_version"], "v1");
}

#[tokio::test]
async fn responses_carry_request_id() {
    let app = common::offline_app();
    let (_, headers, _) = common::send_full(
        &app,
        Request::get("/api/version").body(Body::empty()).unwrap(),
    )
    .await;
    assert!(headers.get("x-request-id").is_some());
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn health_ok_with_database(pool: sqlx::PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let (status, body) =
        common::send(&app, Request::get("/health").body(Body::empty()).unwrap()).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
}

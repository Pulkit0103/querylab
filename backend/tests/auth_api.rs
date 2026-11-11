mod common;

use axum::{body::Body, http::Request};
use common::{api, register, send, send_full};
use serde_json::json;
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Offline: behaviour that must hold without touching the database.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn protected_routes_require_authentication() {
    let app = common::offline_app();
    for (method, uri) in [
        ("GET", "/api/connections"),
        ("GET", "/api/auth/me"),
        ("POST", "/api/auth/logout"),
        (
            "GET",
            "/api/connections/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/api/connections/00000000-0000-0000-0000-000000000000",
        ),
    ] {
        let (status, body) = send(&app, api(method, uri, None, None)).await;
        assert_eq!(status, 401, "{method} {uri}");
        assert_eq!(body["error"]["code"], "unauthorized");
    }
}

#[tokio::test]
async fn state_changing_requests_require_csrf_header() {
    let app = common::offline_app();
    let req = Request::post("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"a@b.co","password":"x"}"#))
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["message"], "missing CSRF header");
}

#[tokio::test]
async fn register_validates_input_before_touching_the_database() {
    let app = common::offline_app();
    let (status, body) = send(
        &app,
        api(
            "POST",
            "/api/auth/register",
            Some(json!({ "email": "nope", "password": "short" })),
            None,
        ),
    )
    .await;
    assert_eq!(status, 422);
    let fields: Vec<_> = body["error"]["details"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["field"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(fields, ["email", "password"]);
}

#[tokio::test]
async fn malformed_json_is_a_400_envelope() {
    let app = common::offline_app();
    let req = Request::post("/api/auth/login")
        .header("content-type", "application/json")
        .header("x-querylab-csrf", "1")
        .body(Body::from("{not json"))
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn cors_preflight_allows_configured_origin_only() {
    let app = common::offline_app();
    let preflight = |origin: &str| {
        Request::builder()
            .method("OPTIONS")
            .uri("/api/connections")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .header(
                "access-control-request-headers",
                "x-querylab-csrf,content-type",
            )
            .body(Body::empty())
            .unwrap()
    };
    let (_, headers, _) = send_full(&app, preflight("http://localhost:3000")).await;
    assert_eq!(
        headers.get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
    let (_, headers, _) = send_full(&app, preflight("https://evil.example")).await;
    assert!(headers.get("access-control-allow-origin").is_none());
}

// ---------------------------------------------------------------------------
// Database-backed flows
// ---------------------------------------------------------------------------

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn register_login_me_logout(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);

    let (status, headers, body) = send_full(
        &app,
        api(
            "POST",
            "/api/auth/register",
            Some(json!({ "email": "Ada@Example.com", "password": "correct horse battery", "display_name": "Ada" })),
            None,
        ),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(body["email"], "Ada@Example.com");
    assert!(body.get("password_hash").is_none());
    let cookie = headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");

    // Duplicate email (case-insensitive) is rejected.
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/auth/register",
            Some(json!({ "email": "ada@example.com", "password": "correct horse battery" })),
            None,
        ),
    )
    .await;
    assert_eq!(status, 409);

    // Wrong password and unknown user produce the same response.
    for (email, password) in [
        ("ada@example.com", "wrong password!"),
        ("nobody@example.com", "whatever12345"),
    ] {
        let (status, body) = send(
            &app,
            api(
                "POST",
                "/api/auth/login",
                Some(json!({ "email": email, "password": password })),
                None,
            ),
        )
        .await;
        assert_eq!(status, 401);
        assert_eq!(body["error"]["code"], "invalid_credentials");
        assert_eq!(body["error"]["message"], "invalid email or password");
    }

    // Login via cookie, then use the cookie for /me.
    let (status, headers, _) = send_full(
        &app,
        api(
            "POST",
            "/api/auth/login",
            Some(json!({ "email": "ADA@example.com", "password": "correct horse battery" })),
            None,
        ),
    )
    .await;
    assert_eq!(status, 200);
    let token = common::session_token(&headers).unwrap();
    let me = Request::get("/api/auth/me")
        .header("cookie", format!("ql_session={token}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, me).await;
    assert_eq!(status, 200);
    assert_eq!(body["display_name"], "Ada");

    // Logout revokes the session server-side.
    let (status, headers, _) =
        send_full(&app, api("POST", "/api/auth/logout", None, Some(&token))).await;
    assert_eq!(status, 204);
    assert!(headers
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));
    let (status, _) = send(&app, api("GET", "/api/auth/me", None, Some(&token))).await;
    assert_eq!(status, 401);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn expired_sessions_are_rejected(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let token = register(&app, "a@example.com").await;
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(&app, api("GET", "/api/auth/me", None, Some(&token))).await;
    assert_eq!(status, 401);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn sessions_store_only_token_hashes(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let token = register(&app, "a@example.com").await;
    let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 32);
    assert!(!stored
        .windows(token.len().min(8))
        .any(|w| w == &token.as_bytes()[..w.len()]));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn login_is_rate_limited(pool: PgPool) {
    let (app, _) = common::app_with_config(pool, |c| c.auth_rate_limit_per_minute = 3);
    let attempt = || {
        api(
            "POST",
            "/api/auth/login",
            Some(json!({ "email": "x@example.com", "password": "whatever12345" })),
            None,
        )
    };
    for _ in 0..3 {
        assert_eq!(send(&app, attempt()).await.0, 401);
    }
    let (status, headers, body) = send_full(&app, attempt()).await;
    assert_eq!(status, 429);
    assert_eq!(body["error"]["code"], "rate_limited");
    assert!(headers.get("retry-after").is_some());
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn connection_crud_is_scoped_to_owner(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let alice = register(&app, "alice@example.com").await;
    let bob = register(&app, "bob@example.com").await;

    let (status, created) = send(
        &app,
        api(
            "POST",
            "/api/connections",
            Some(json!({
                "name": "Analytics", "host": "db.example.com", "port": 5432,
                "database": "analytics", "username": "reader", "password": "p@ss-w0rd!",
                "ssl_mode": "verify-full"
            })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["read_only"], true);
    assert_eq!(created["ssl_mode"], "verify-full");
    assert!(
        !created.to_string().contains("p@ss-w0rd!"),
        "password must never be returned"
    );
    let id = created["id"].as_str().unwrap().to_string();
    let path = format!("/api/connections/{id}");

    // Owner can read, list and update.
    assert_eq!(
        send(&app, api("GET", &path, None, Some(&alice))).await.0,
        200
    );
    let (_, list) = send(&app, api("GET", "/api/connections", None, Some(&alice))).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, updated) = send(
        &app,
        api(
            "PUT",
            &path,
            Some(json!({ "name": "Analytics (ro)" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(updated["name"], "Analytics (ro)");

    // Another user sees nothing and cannot read, modify or delete.
    let (_, list) = send(&app, api("GET", "/api/connections", None, Some(&bob))).await;
    assert!(list.as_array().unwrap().is_empty());
    assert_eq!(send(&app, api("GET", &path, None, Some(&bob))).await.0, 404);
    assert_eq!(
        send(
            &app,
            api("PUT", &path, Some(json!({ "name": "pwned" })), Some(&bob))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        send(&app, api("DELETE", &path, None, Some(&bob))).await.0,
        404
    );

    // Invalid ids and bodies are rejected with envelopes.
    let (status, body) = send(
        &app,
        api("GET", "/api/connections/not-a-uuid", None, Some(&alice)),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "bad_request");
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/connections",
            Some(json!({ "name": "x", "host": "http://evil", "database": "d", "username": "u" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 422);

    // Owner deletes.
    assert_eq!(
        send(&app, api("DELETE", &path, None, Some(&alice))).await.0,
        204
    );
    assert_eq!(
        send(&app, api("GET", &path, None, Some(&alice))).await.0,
        404
    );
}

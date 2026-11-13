//! Cross-cutting security and robustness checks: response hardening,
//! request validation, session handling, isolation, execution limits and
//! behaviour under concurrent requests.

mod common;

use axum::{body::Body, http::Request};
use common::{api, register, sample_connection, send, send_full};
use futures_util::future::join_all;
use serde_json::{json, Value};
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Offline (no database)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_response_carries_security_headers() {
    let app = common::offline_app();
    for req in [
        Request::get("/health").body(Body::empty()).unwrap(),
        Request::get("/api/nope").body(Body::empty()).unwrap(),
        api("GET", "/api/connections", None, None),
    ] {
        let (_, headers, _) = send_full(&app, req).await;
        assert_eq!(headers["x-content-type-options"], "nosniff");
        assert_eq!(headers["x-frame-options"], "DENY");
        assert_eq!(headers["referrer-policy"], "no-referrer");
        assert_eq!(headers["cache-control"], "no-store");
        assert!(headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'none'"));
        assert!(headers.get("x-request-id").is_some());
        // Plain HTTP (development): no HSTS.
        assert!(headers.get("strict-transport-security").is_none());
    }
}

#[tokio::test]
async fn hsts_is_sent_when_serving_over_https() {
    let mut config = common::offline_config();
    config.cookie_secure = true;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy(&config.database_url)
        .unwrap();
    let app = querylab_api::build_router(querylab_api::AppState::new(config, pool));
    let (_, headers, _) =
        send_full(&app, Request::get("/health").body(Body::empty()).unwrap()).await;
    assert!(headers["strict-transport-security"]
        .to_str()
        .unwrap()
        .starts_with("max-age="));
}

#[tokio::test]
async fn oversized_bodies_are_rejected() {
    let app = common::offline_app();
    let huge = "x".repeat(3 * 1024 * 1024);
    let req = Request::post("/api/auth/login")
        .header("x-querylab-csrf", "1")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": "a@example.com", "password": huge }).to_string(),
        ))
        .unwrap();
    let (status, _, body) = send_full(&app, req).await;
    assert_eq!(status, 413);
    assert_eq!(body["error"]["code"], "payload_too_large");
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn forged_and_tampered_tokens_are_rejected(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let mut tampered = token.clone();
    let last = tampered.pop().unwrap();
    tampered.push(if last == 'A' { 'B' } else { 'A' });

    for bad in [
        tampered.as_str(),
        "not-a-token",
        "",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let (status, body) = send(&app, api("GET", "/api/auth/me", None, Some(bad))).await;
        assert_eq!(status, 401, "token {bad:?}");
        assert_eq!(body["error"]["code"], "unauthorized");
    }
    // The genuine token still works.
    assert_eq!(
        send(&app, api("GET", "/api/auth/me", None, Some(&token)))
            .await
            .0,
        200
    );
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn each_login_gets_an_independent_session(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let first = register(&app, "a@example.com").await;
    let (status, headers, _) = send_full(
        &app,
        api(
            "POST",
            "/api/auth/login",
            Some(json!({ "email": "a@example.com", "password": "correct horse battery" })),
            None,
        ),
    )
    .await;
    assert_eq!(status, 200);
    let second = common::session_token(&headers).unwrap();
    assert_ne!(first, second, "login must issue a fresh token");

    // Logging out one device leaves the other signed in.
    send(&app, api("POST", "/api/auth/logout", None, Some(&first))).await;
    assert_eq!(
        send(&app, api("GET", "/api/auth/me", None, Some(&first)))
            .await
            .0,
        401
    );
    assert_eq!(
        send(&app, api("GET", "/api/auth/me", None, Some(&second)))
            .await
            .0,
        200
    );
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn concurrent_registrations_create_exactly_one_account(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let attempts = (0..8).map(|_| {
        send(
            &app,
            api(
                "POST",
                "/api/auth/register",
                Some(json!({ "email": "race@example.com", "password": "correct horse battery" })),
                None,
            ),
        )
    });
    let statuses: Vec<u16> = join_all(attempts)
        .await
        .into_iter()
        .map(|(s, _)| s.as_u16())
        .collect();
    assert_eq!(
        statuses.iter().filter(|s| **s == 201).count(),
        1,
        "{statuses:?}"
    );
    assert!(
        statuses.iter().all(|s| *s == 201 || *s == 409),
        "{statuses:?}"
    );
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 1);
}

// ---------------------------------------------------------------------------
// Validation and isolation
// ---------------------------------------------------------------------------

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn malformed_identifiers_and_payloads_get_error_envelopes(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;

    let (status, body) = send(
        &app,
        api("GET", "/api/connections/not-a-uuid", None, Some(&token)),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(body["error"]["code"].is_string());

    let too_long = "SELECT 1 -- ".to_string() + &"x".repeat(1024 * 1024);
    let (status, body) = send(
        &app,
        api(
            "POST",
            "/api/query/execute",
            Some(json!({ "connection_id": uuid::Uuid::new_v4(), "query": too_long })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["error"]["details"]["fields"][0]["field"], "query");

    let (status, body) = send(
        &app,
        api(
            "POST",
            "/api/query/execute",
            Some(json!({ "connection_id": uuid::Uuid::new_v4(), "query": "select 1", "max_rows": 0 })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 422, "{body}");

    // Unknown fields with the wrong type are rejected, not ignored silently.
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/connections",
            Some(json!({ "name": "x", "host": "db.example.com", "port": "not a port", "database": "d", "username": "u", "password": "p" })),
            Some(&token),
        ),
    )
    .await;
    assert!(status == 400 || status == 422, "{status}");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn credentials_never_appear_in_responses(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let (_, created) = send(
        &app,
        api(
            "POST",
            "/api/connections",
            Some(sample_connection("Sample")),
            Some(&token),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();

    let mut bodies: Vec<Value> = vec![created];
    for path in [
        "/api/connections".to_string(),
        format!("/api/connections/{id}"),
    ] {
        bodies.push(send(&app, api("GET", &path, None, Some(&token))).await.1);
    }
    bodies.push(
        send(
            &app,
            api(
                "POST",
                &format!("/api/connections/{id}/test"),
                None,
                Some(&token),
            ),
        )
        .await
        .1,
    );
    for body in bodies {
        let text = body.to_string();
        assert!(!text.contains("reader_password"), "{text}");
        assert!(!text.contains("\"password"), "{text}");
        assert!(!text.contains("encrypted_"), "{text}");
    }
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn other_users_cannot_touch_any_connection_endpoint(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let alice = register(&app, "alice@example.com").await;
    let mallory = register(&app, "mallory@example.com").await;
    let (_, created) = send(
        &app,
        api(
            "POST",
            "/api/connections",
            Some(sample_connection("Alice DB")),
            Some(&alice),
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    let attempts: Vec<(&str, String, Option<Value>)> = vec![
        ("GET", format!("/api/connections/{id}"), None),
        (
            "PUT",
            format!("/api/connections/{id}"),
            Some(json!({ "name": "pwned" })),
        ),
        ("POST", format!("/api/connections/{id}/test"), None),
        ("GET", format!("/api/connections/{id}/schemas"), None),
        (
            "GET",
            format!("/api/connections/{id}/tables?schema=public"),
            None,
        ),
        ("GET", format!("/api/connections/{id}/catalog"), None),
        (
            "GET",
            format!("/api/connections/{id}/tables/employees/columns?schema=public"),
            None,
        ),
        (
            "POST",
            "/api/query/execute".into(),
            Some(json!({ "connection_id": id, "query": "select 1" })),
        ),
        (
            "POST",
            "/api/query/explain".into(),
            Some(json!({ "connection_id": id, "query": "select 1" })),
        ),
        ("DELETE", format!("/api/connections/{id}"), None),
    ];
    for (method, path, body) in attempts {
        let (status, res) = send(&app, api(method, &path, body, Some(&mallory))).await;
        assert_eq!(status, 404, "{method} {path}: {res}");
    }
    // Still intact for the owner.
    let (status, res) = send(
        &app,
        api("GET", &format!("/api/connections/{id}"), None, Some(&alice)),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["name"], "Alice DB");
}

// ---------------------------------------------------------------------------
// Execution limits under concurrency
// ---------------------------------------------------------------------------

async fn connection_for(app: &axum::Router, token: &str) -> String {
    let (status, created) = send(
        app,
        api(
            "POST",
            "/api/connections",
            Some(sample_connection("Sample")),
            Some(token),
        ),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    created["id"].as_str().unwrap().to_string()
}

fn run(id: &str, token: &str, sql: &str) -> Request<Body> {
    api(
        "POST",
        "/api/query/execute",
        Some(json!({ "connection_id": id, "query": sql })),
        Some(token),
    )
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn concurrent_executions_are_capped_per_user(pool: PgPool) {
    let (app, _) = common::app_with_config(pool, |c| {
        c.max_concurrent_queries_per_user = 2;
        // Bob's query must not fail on connect latency when the test
        // database server is busy with parallel suites.
        c.target_connect_timeout = std::time::Duration::from_secs(30);
    });
    let alice = register(&app, "alice@example.com").await;
    let bob = register(&app, "bob@example.com").await;
    let alice_conn = connection_for(&app, &alice).await;
    let bob_conn = connection_for(&app, &bob).await;

    // Alice saturates her slots; Bob's query runs alongside unaffected.
    let alice_runs =
        join_all((0..4).map(|_| send(&app, run(&alice_conn, &alice, "SELECT pg_sleep(1.5)"))));
    let bob_run = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        send(&app, run(&bob_conn, &bob, "SELECT 1 AS ok")).await
    };
    let (alice_results, (bob_status, bob_body)) = tokio::join!(alice_runs, bob_run);

    let ok = alice_results.iter().filter(|(s, _)| *s == 200).count();
    let limited: Vec<&Value> = alice_results
        .iter()
        .filter(|(s, _)| *s == 429)
        .map(|(_, b)| b)
        .collect();
    assert_eq!(ok, 2, "{alice_results:?}");
    assert_eq!(limited.len(), 2, "{alice_results:?}");
    assert!(limited.iter().all(|b| b["error"]["code"] == "rate_limited"));
    assert_eq!(bob_status, 200, "{bob_body}");

    // Slots are released once executions finish.
    assert_eq!(
        send(&app, run(&alice_conn, &alice, "SELECT 1")).await.0,
        200
    );
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn query_rate_limit_is_per_user(pool: PgPool) {
    let (app, _) = common::app_with_config(pool, |c| c.query_rate_limit_per_minute = 2);
    let alice = register(&app, "alice@example.com").await;
    let bob = register(&app, "bob@example.com").await;
    let alice_conn = connection_for(&app, &alice).await;
    let bob_conn = connection_for(&app, &bob).await;

    for _ in 0..2 {
        assert_eq!(
            send(&app, run(&alice_conn, &alice, "SELECT 1")).await.0,
            200
        );
    }
    let (status, headers, body) = send_full(&app, run(&alice_conn, &alice, "SELECT 1")).await;
    assert_eq!(status, 429, "{body}");
    assert!(headers.get("retry-after").is_some());
    // Explain counts against the same budget.
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/query/explain",
            Some(json!({ "connection_id": alice_conn, "query": "SELECT 1" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 429);
    assert_eq!(send(&app, run(&bob_conn, &bob, "SELECT 1")).await.0, 200);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn row_limits_are_clamped_to_the_server_maximum(pool: PgPool) {
    let (app, _) = common::app_with_config(pool, |c| c.query_max_rows = 50);
    let token = register(&app, "a@example.com").await;
    let id = connection_for(&app, &token).await;
    let (status, body) = send(
        &app,
        api(
            "POST",
            "/api/query/execute",
            Some(json!({ "connection_id": id, "query": "SELECT g FROM generate_series(1, 1000) g", "max_rows": 100000 })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["row_count"], 50);
    assert_eq!(body["truncated"], true);
}

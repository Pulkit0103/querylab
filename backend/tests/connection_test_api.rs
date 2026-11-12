//! Connection testing against the seeded sample database, plus SSRF policy.
//! Requires DATABASE_URL and the sample DB from `scripts/sample-db`.

mod common;

use common::{api, register, sample_connection, send};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn draft_connection_test_reports_server_facts(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;

    let (status, body) = send(
        &app,
        api(
            "POST",
            "/api/connections/test",
            Some(sample_connection("Sample")),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["database"], "sampledb");
    assert_eq!(body["user"], "querylab_reader");
    assert_eq!(body["is_superuser"], false);
    assert_eq!(body["read_only"], true);
    assert!(body["server_version"].as_str().unwrap().len() >= 2);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn saved_connection_test_updates_last_connected(pool: PgPool) {
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
    assert!(created["last_connected_at"].is_null());
    let id = created["id"].as_str().unwrap();

    let (status, body) = send(
        &app,
        api(
            "POST",
            &format!("/api/connections/{id}/test"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let (_, fetched) = send(
        &app,
        api("GET", &format!("/api/connections/{id}"), None, Some(&token)),
    )
    .await;
    assert!(!fetched["last_connected_at"].is_null());

    // Another user cannot test (or learn anything about) this connection.
    let other = register(&app, "b@example.com").await;
    let (status, _) = send(
        &app,
        api(
            "POST",
            &format!("/api/connections/{id}/test"),
            None,
            Some(&other),
        ),
    )
    .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn connection_failures_are_reported_without_secrets(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;

    let mut bad_password = sample_connection("x");
    bad_password["password"] = json!("definitely-wrong-pw");
    let (status, body) = send(
        &app,
        api(
            "POST",
            "/api/connections/test",
            Some(bad_password),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 502);
    assert_eq!(body["error"]["code"], "connection_error");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("password authentication failed"),
        "{message}"
    );
    assert!(!body.to_string().contains("definitely-wrong-pw"));

    let mut bad_db = sample_connection("x");
    bad_db["database"] = json!("no_such_db");
    let (status, body) = send(
        &app,
        api("POST", "/api/connections/test", Some(bad_db), Some(&token)),
    )
    .await;
    assert_eq!(status, 502);
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no_such_db"));

    let mut refused = sample_connection("x");
    refused["port"] = json!(59999);
    let (status, body) = send(
        &app,
        api("POST", "/api/connections/test", Some(refused), Some(&token)),
    )
    .await;
    assert_eq!(status, 502);
    assert!(body["error"]["message"].as_str().unwrap().contains("59999"));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn private_destinations_are_blocked_by_default(pool: PgPool) {
    let (app, _) = common::app_with_config(pool, |c| c.allow_private_networks = false);
    let token = register(&app, "a@example.com").await;

    for host in [
        "127.0.0.1",
        "localhost",
        "169.254.169.254",
        "10.0.0.5",
        "[::1]",
        "db.internal",
    ] {
        let mut body = sample_connection("x");
        body["host"] = json!(host);
        let (status, res) = send(
            &app,
            api("POST", "/api/connections", Some(body.clone()), Some(&token)),
        )
        .await;
        assert_eq!(status, 422, "create {host}: {res}");
        let (status, _) = send(
            &app,
            api("POST", "/api/connections/test", Some(body), Some(&token)),
        )
        .await;
        assert_eq!(status, 422, "test {host}");
    }

    let mut low_port = sample_connection("x");
    low_port["host"] = json!("db.example.com");
    low_port["port"] = json!(22);
    let (status, res) = send(
        &app,
        api("POST", "/api/connections", Some(low_port), Some(&token)),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(res["error"]["details"]["fields"][0]["field"], "port");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn connection_tests_are_rate_limited_per_user(pool: PgPool) {
    let (app, _) = common::app_with_config(pool, |c| c.target_connect_rate_limit_per_minute = 2);
    let token = register(&app, "a@example.com").await;
    let mut body = sample_connection("x");
    body["port"] = json!(59999);
    for _ in 0..2 {
        let (status, _) = send(
            &app,
            api(
                "POST",
                "/api/connections/test",
                Some(body.clone()),
                Some(&token),
            ),
        )
        .await;
        assert_eq!(status, 502);
    }
    let (status, _) = send(
        &app,
        api("POST", "/api/connections/test", Some(body), Some(&token)),
    )
    .await;
    assert_eq!(status, 429);
}

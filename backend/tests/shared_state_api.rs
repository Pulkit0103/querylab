//! Phase 10c: two API instances sharing Redis share rate limits and
//! concurrency limits, and either one can cancel a query running on the
//! other. Skipped when `REDIS_URL` is not set (CI provides Redis).

mod common;

use std::time::{Duration, Instant};

use axum::Router;
use common::{api, register, sample_connection, send};
use querylab_api::{services::shared_state::SharedStore, AppState};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn redis_url() -> Option<String> {
    std::env::var("REDIS_URL").ok().filter(|u| !u.is_empty())
}

/// Two instances over the same metadata database and Redis prefix.
async fn two_instances(
    pool: PgPool,
    url: &str,
    tweak: impl Fn(&mut querylab_api::config::Config) + Copy,
) -> ((Router, AppState), (Router, AppState)) {
    let prefix = format!("ql-test-{}", Uuid::new_v4().simple());
    let store = SharedStore::connect(url, &prefix)
        .await
        .expect("connect to Redis");
    let a = common::app_with_config(pool.clone(), tweak);
    let b = common::app_with_config(pool, tweak);
    a.1.attach_shared(store.clone());
    b.1.attach_shared(store);
    (a, b)
}

async fn call(
    app: &Router,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let (status, json) = send(app, api(method, path, body, Some(token))).await;
    (status.as_u16(), json)
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and REDIS_URL"]
async fn rate_limits_are_shared(pool: PgPool) {
    let Some(url) = redis_url() else {
        eprintln!("skipped: REDIS_URL is not set");
        return;
    };
    let ((a, _), (b, _)) = two_instances(pool, &url, |c| c.query_rate_limit_per_minute = 3).await;
    // A session created on one instance is valid on the other.
    let token = register(&a, "limits@example.com").await;
    let missing = json!({ "connection_id": Uuid::new_v4(), "query": "select 1" });
    // Each attempt counts against the user's window before the connection
    // lookup fails, wherever it lands.
    for app in [&a, &b, &a] {
        let (status, _) = call(
            app,
            &token,
            "POST",
            "/api/query/execute",
            Some(missing.clone()),
        )
        .await;
        assert_eq!(status, 404);
    }
    let (status, body) = call(&b, &token, "POST", "/api/query/execute", Some(missing)).await;
    assert_eq!(status, 429, "{body}");
    assert!(
        body["error"]["details"]["retry_after_secs"]
            .as_u64()
            .unwrap()
            >= 1
    );

    let (status, health) = send(&b, api("GET", "/health", None, None)).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(health["shared_state"], "ok");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL, REDIS_URL and the sample database"]
async fn any_instance_cancels_any_query(pool: PgPool) {
    let Some(url) = redis_url() else {
        eprintln!("skipped: REDIS_URL is not set");
        return;
    };
    let ((a, a_state), (b, _)) =
        two_instances(pool, &url, |c| c.max_concurrent_queries_per_user = 1).await;
    let token = register(&a, "cancel@example.com").await;
    let (status, conn) = call(
        &a,
        &token,
        "POST",
        "/api/connections",
        Some(sample_connection("Sample")),
    )
    .await;
    assert_eq!(status, 201, "{conn}");
    let connection_id = conn["id"].as_str().unwrap().to_string();

    // A long query on instance A.
    let execution_id = Uuid::new_v4();
    let started = Instant::now();
    let long = {
        let (a, token) = (a.clone(), token.clone());
        let body = json!({
            "connection_id": connection_id, "query": "select pg_sleep(30)",
            "execution_id": execution_id, "timeout_secs": 50,
        });
        tokio::spawn(
            async move { call(&a, &token, "POST", "/api/query/execute", Some(body)).await },
        )
    };

    // Wait until A has registered the execution: probing from B earlier
    // could take the only slot and make A's query the one rejected.
    for _ in 0..200 {
        if a_state.executions.running() > 0 || long.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if long.is_finished() {
        panic!("the long query ended early: {:?}", long.await.unwrap());
    }
    assert_eq!(
        a_state.executions.running(),
        1,
        "A never registered the query"
    );

    // Instance B sees the global concurrency limit.
    let quick = json!({ "connection_id": connection_id, "query": "select 1" });
    let (status, body) = call(
        &b,
        &token,
        "POST",
        "/api/query/execute",
        Some(quick.clone()),
    )
    .await;
    assert_eq!(
        status, 429,
        "the second instance never saw the running query: {body}"
    );
    // Let A open its connection and publish the backend pid.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Instance B cancels the query running on A; other users cannot.
    let other = register(&b, "other@example.com").await;
    let (status, _) = call(
        &b,
        &other,
        "POST",
        &format!("/api/query/{execution_id}/cancel"),
        None,
    )
    .await;
    assert_eq!(status, 404);
    let (status, body) = call(
        &b,
        &token,
        "POST",
        &format!("/api/query/{execution_id}/cancel"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["cancelled"], true);

    let (status, body) = long.await.unwrap();
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"]["code"], "query_cancelled");
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "cancellation took {:?}",
        started.elapsed()
    );

    // The slot is free again on both instances.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, _) = call(&b, &token, "POST", "/api/query/execute", Some(quick)).await;
    assert_eq!(status, 200);
}

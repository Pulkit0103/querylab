//! Phase 7 performance history (fingerprints) and saved plans.

mod common;

use axum::Router;
use common::{api, register, sample_connection, send};
use querylab_api::utils::fingerprint::fingerprint;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

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

async fn connection(app: &Router, token: &str, name: &str) -> String {
    let (status, created) = call(
        app,
        token,
        "POST",
        "/api/connections",
        Some(sample_connection(name)),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    created["id"].as_str().unwrap().to_string()
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn fingerprints_group_equivalent_queries(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let token = register(&app, "a@example.com").await;
    let conn = connection(&app, &token, "Sample").await;
    let other = connection(&app, &token, "Other").await;

    let run = |sql: &'static str| {
        let app = app.clone();
        let token = token.clone();
        let conn = conn.clone();
        async move {
            call(
                &app,
                &token,
                "POST",
                "/api/query/execute",
                Some(json!({ "connection_id": conn, "query": sql })),
            )
            .await
        }
    };
    for sql in [
        "SELECT id FROM public.departments WHERE id = 1",
        "select id from PUBLIC.departments where id = 2;",
        "SELECT id FROM public.departments /* again */ WHERE id = 3",
        // Same shape, fails at runtime.
        "SELECT id FROM public.departments WHERE id = 'x'",
        "SELECT count(*) FROM sales.orders",
    ] {
        run(sql).await;
    }

    let (status, list) = call(&app, &token, "GET", "/api/performance/fingerprints", None).await;
    assert_eq!(status, 200, "{list}");
    let groups = list.as_array().unwrap();
    let dept = groups
        .iter()
        .find(|g| g["normalized_sql"] == "select id from public.departments where id = ?")
        .unwrap_or_else(|| panic!("{list}"));
    assert_eq!(dept["executions"], 4);
    assert_eq!(dept["errors"], 1);
    assert!(dept["p50_ms"].is_number() && dept["p95_ms"].is_number());
    assert_eq!(dept["connection_name"], "Sample");
    assert!(dept["sample_sql"].as_str().unwrap().contains("'x'"));
    let fp = dept["fingerprint"].as_str().unwrap().to_string();
    assert_eq!(
        fp,
        fingerprint("SELECT id FROM public.departments WHERE id = 1").to_string()
    );
    assert!(groups
        .iter()
        .any(|g| g["normalized_sql"] == "select count(*) from sales.orders"));

    let (status, timeline) = call(
        &app,
        &token,
        "GET",
        &format!("/api/performance/fingerprints/{fp}?days=7"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{timeline}");
    let points = timeline["points"].as_array().unwrap();
    assert_eq!(points.len(), 7);
    assert_eq!(points.last().unwrap()["executions"], 4);
    assert_eq!(points[0]["executions"], 0);
    assert_eq!(timeline["recent"].as_array().unwrap().len(), 4);
    assert_eq!(timeline["stats"]["executions"], 4);

    // Filters and errors.
    let (_, none) = call(
        &app,
        &token,
        "GET",
        &format!("/api/performance/fingerprints?connection_id={other}"),
        None,
    )
    .await;
    assert_eq!(none, json!([]));
    assert_eq!(
        call(
            &app,
            &token,
            "GET",
            "/api/performance/fingerprints?days=0",
            None
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            &app,
            &token,
            "GET",
            "/api/performance/fingerprints/abc",
            None
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            &app,
            &token,
            "GET",
            "/api/performance/fingerprints/42",
            None
        )
        .await
        .0,
        404
    );
    let mallory = register(&app, "m@example.com").await;
    assert_eq!(
        call(
            &app,
            &mallory,
            "GET",
            &format!("/api/performance/fingerprints/{fp}"),
            None
        )
        .await
        .0,
        404
    );

    // History recorded before fingerprints is backfilled.
    let id = Uuid::new_v4();
    let me: Value = call(&app, &token, "GET", "/api/auth/me", None).await.1;
    sqlx::query(
        "INSERT INTO query_history (id, user_id, sql_content, execution_time_ms, status) \
         VALUES ($1, $2, 'SELECT 42 AS answer', 3, 'success')",
    )
    .bind(id)
    .bind(Uuid::parse_str(me["id"].as_str().unwrap()).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let updated = querylab_api::services::library::backfill_fingerprints(&pool)
        .await
        .unwrap();
    assert!(updated >= 1);
    let stored: Option<i64> =
        sqlx::query_scalar("SELECT fingerprint FROM query_history WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, Some(fingerprint("select 7 as answer")));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn plans_are_saved_for_comparison(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let conn = connection(&app, &token, "Sample").await;
    let sql = "SELECT * FROM sales.orders WHERE customer_id IS NOT NULL";
    let (status, plan) = call(
        &app,
        &token,
        "POST",
        "/api/query/explain",
        Some(json!({ "connection_id": conn, "query": sql })),
    )
    .await;
    assert_eq!(status, 200, "{plan}");

    let (status, saved) = call(
        &app,
        &token,
        "POST",
        "/api/plans",
        Some(json!({ "name": " Before index ", "connection_id": conn, "sql": sql, "plan": plan })),
    )
    .await;
    assert_eq!(status, 201, "{saved}");
    assert_eq!(saved["name"], "Before index");
    assert_eq!(saved["analyzed"], false);
    assert_eq!(
        saved["plan"]["plan"]["Node Type"],
        plan["plan"]["Node Type"]
    );
    let id = saved["id"].as_str().unwrap().to_string();

    let (_, list) = call(&app, &token, "GET", "/api/plans", None).await;
    let item = &list[0];
    assert_eq!(item["id"], id.as_str());
    assert_eq!(item["connection_name"], "Sample");
    assert!(item["total_cost"].as_f64().unwrap() > 0.0);
    assert_eq!(item["execution_time_ms"], Value::Null);
    assert!(item["sql_preview"]
        .as_str()
        .unwrap()
        .starts_with("SELECT * FROM sales.orders"));

    // Validation and ownership.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        "/api/plans",
        Some(json!({ "name": "x", "sql": sql, "plan": { "nope": 1 } })),
    )
    .await;
    assert_eq!(status, 422);
    let mallory = register(&app, "m@example.com").await;
    assert_eq!(
        call(&app, &mallory, "GET", &format!("/api/plans/{id}"), None)
            .await
            .0,
        404
    );
    assert_eq!(
        call(&app, &mallory, "DELETE", &format!("/api/plans/{id}"), None)
            .await
            .0,
        404
    );
    let (status, _) = call(
        &app,
        &mallory,
        "POST",
        "/api/plans",
        Some(json!({ "name": "steal", "connection_id": conn, "sql": sql, "plan": plan })),
    )
    .await;
    assert_eq!(status, 404);

    assert_eq!(
        call(&app, &token, "DELETE", &format!("/api/plans/{id}"), None)
            .await
            .0,
        204
    );
    assert_eq!(
        call(&app, &token, "GET", &format!("/api/plans/{id}"), None)
            .await
            .0,
        404
    );
}

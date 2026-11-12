//! SQL execution engine against the seeded sample database.

mod common;

use std::time::Duration;

use common::{api, register, sample_connection, send};
use serde_json::{json, Value};
use sqlx::PgPool;

async fn setup_with(pool: PgPool, read_only: bool, owner: bool) -> (axum::Router, String, String) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let mut body = sample_connection("Sample");
    body["read_only"] = json!(read_only);
    if owner {
        // The table owner can write; used to prove the read-only transaction
        // (not role privileges) is what blocks writes.
        body["username"] = json!("sample_owner");
        body["password"] = json!("sample_owner");
    }
    let (status, created) = send(
        &app,
        api("POST", "/api/connections", Some(body), Some(&token)),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    (app, token, created["id"].as_str().unwrap().to_string())
}

async fn exec(
    app: &axum::Router,
    token: &str,
    id: &str,
    sql: &str,
    extra: Value,
) -> (axum::http::StatusCode, Value) {
    let mut body = json!({ "connection_id": id, "query": sql });
    if let (Some(obj), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        obj.extend(extra.clone());
    }
    send(
        app,
        api("POST", "/api/query/execute", Some(body), Some(token)),
    )
    .await
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn executes_select_with_typed_values(pool: PgPool) {
    let (app, token, id) = setup_with(pool, true, false).await;
    let (status, res) = exec(
        &app,
        &token,
        &id,
        "SELECT 1::int4 AS i, 9007199254740993::int8 AS big, 12345.678::numeric(10,3) AS n, \
                'hi'::text AS t, true AS b, NULL::text AS nothing, \
                '{\"a\": [1, 2]}'::jsonb AS j, '2024-02-29'::date AS d, \
                '2024-01-02 03:04:05+00'::timestamptz AS ts, interval '1 day 2 hours' AS iv, \
                '\\xdead'::bytea AS bin, ARRAY[1, NULL, 3] AS arr, \
                'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid AS u, '10.0.0.0/8'::cidr AS net, \
                'infinity'::timestamptz AS inf, 'NaN'::float8 AS nan",
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{res}");
    assert_eq!(res["status"], "success");
    assert_eq!(res["command"], "SELECT");
    assert_eq!(res["row_count"], 1);
    assert_eq!(res["truncated"], false);
    let cols: Vec<_> = res["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            format!(
                "{}:{}",
                c["name"].as_str().unwrap(),
                c["type"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(cols[0], "i:int4");
    assert_eq!(cols[2], "n:numeric");
    let row = &res["rows"][0];
    assert_eq!(row[0], json!(1));
    assert_eq!(row[1], json!("9007199254740993"), "unsafe int8 as string");
    assert_eq!(row[2], json!("12345.678"));
    assert_eq!(row[3], json!("hi"));
    assert_eq!(row[4], json!(true));
    assert_eq!(row[5], Value::Null);
    assert_eq!(row[6], json!({"a": [1, 2]}));
    assert_eq!(row[7], json!("2024-02-29"));
    assert_eq!(row[8], json!("2024-01-02T03:04:05+00:00"));
    assert_eq!(row[9], json!("1 day 02:00:00"));
    assert_eq!(row[10], json!("\\xdead"));
    assert_eq!(row[11], json!([1, null, 3]));
    assert_eq!(row[12], json!("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"));
    assert_eq!(row[13], json!("10.0.0.0/8"));
    assert_eq!(row[14], json!("infinity"));
    assert_eq!(row[15], json!("NaN"));
    assert!(res["execution_time_ms"].as_u64().is_some());
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn returns_columns_for_empty_results_and_truncates(pool: PgPool) {
    let (app, token, id) = setup_with(pool, true, false).await;
    let (status, res) = exec(
        &app,
        &token,
        &id,
        "SELECT id, name FROM departments WHERE false",
        json!({}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["row_count"], 0);
    assert_eq!(res["columns"].as_array().unwrap().len(), 2);

    let (status, res) = exec(
        &app,
        &token,
        &id,
        "SELECT * FROM sales.orders",
        json!({ "max_rows": 25 }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["row_count"], 25);
    assert_eq!(res["truncated"], true);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn read_only_mode_is_enforced_by_postgres(pool: PgPool) {
    // Connect as the table *owner* so only the read-only transaction can block writes.
    let (app, token, id) = setup_with(pool, true, true).await;
    for sql in [
        "DELETE FROM public.departments",
        "UPDATE public.employees SET salary = 0",
        "CREATE TABLE evil (id int)",
        "SELECT nextval('departments_id_seq')",
    ] {
        let (status, res) = exec(&app, &token, &id, sql, json!({})).await;
        assert_eq!(status, 400, "{sql}: {res}");
        assert_eq!(res["error"]["code"], "sql_error");
        assert_eq!(res["error"]["details"]["sqlstate"], "25006", "{sql}");
    }

    // Escaping the transaction via multiple statements is rejected by the protocol.
    let (status, res) = exec(
        &app,
        &token,
        &id,
        "COMMIT; DELETE FROM public.departments",
        json!({}),
    )
    .await;
    assert_eq!(status, 400, "{res}");
    let (status, res) = exec(
        &app,
        &token,
        &id,
        "SET default_transaction_read_only = off; DELETE FROM public.departments",
        json!({}),
    )
    .await;
    assert_eq!(status, 400, "{res}");

    // Data is intact.
    let (_, res) = exec(
        &app,
        &token,
        &id,
        "SELECT count(*) FROM public.departments",
        json!({}),
    )
    .await;
    assert_eq!(res["rows"][0][0], json!(5));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn read_write_mode_commits_dml(pool: PgPool) {
    let (app, token, id) = setup_with(pool, false, true).await;
    let table = format!("ql_test_{}", uuid::Uuid::new_v4().simple());
    let (status, res) = exec(
        &app,
        &token,
        &id,
        &format!("CREATE TABLE {table} (id int)"),
        json!({}),
    )
    .await;
    assert_eq!(status, 200, "{res}");
    assert_eq!(res["columns"].as_array().unwrap().len(), 0);

    let (status, res) = exec(
        &app,
        &token,
        &id,
        &format!("INSERT INTO {table} SELECT generate_series(1, 30)"),
        json!({}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["command"], "INSERT");
    assert_eq!(res["rows_affected"], 30);

    // RETURNING larger than the row limit must still commit every row.
    let (status, res) = exec(
        &app,
        &token,
        &id,
        &format!("UPDATE {table} SET id = id + 100 RETURNING id"),
        json!({ "max_rows": 5 }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["row_count"], 5);
    assert_eq!(res["truncated"], true);
    let (_, res) = exec(
        &app,
        &token,
        &id,
        &format!("SELECT min(id), count(*) FROM {table}"),
        json!({}),
    )
    .await;
    assert_eq!(res["rows"][0], json!([101, 30]));

    let (status, _) = exec(&app, &token, &id, &format!("DROP TABLE {table}"), json!({})).await;
    assert_eq!(status, 200);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn sql_errors_are_structured(pool: PgPool) {
    let (app, token, id) = setup_with(pool, true, false).await;
    let (status, res) = exec(&app, &token, &id, "SELECT * FROM no_such_table", json!({})).await;
    assert_eq!(status, 400);
    assert_eq!(res["error"]["code"], "sql_error");
    assert_eq!(res["error"]["details"]["sqlstate"], "42P01");
    assert_eq!(res["error"]["details"]["position"], 15);
    assert!(res["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no_such_table"));

    let (status, res) = exec(&app, &token, &id, "SELEC 1", json!({})).await;
    assert_eq!(status, 400);
    assert_eq!(res["error"]["details"]["sqlstate"], "42601");

    // Permission errors come from role privileges.
    let (status, res) = exec(&app, &token, &id, "SELECT * FROM pg_authid", json!({})).await;
    assert_eq!(status, 400);
    assert_eq!(res["error"]["details"]["sqlstate"], "42501");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn timeouts_and_cancellation(pool: PgPool) {
    let (app, token, id) = setup_with(pool, true, false).await;
    let (status, res) = exec(
        &app,
        &token,
        &id,
        "SELECT pg_sleep(5)",
        json!({ "timeout_secs": 1 }),
    )
    .await;
    assert_eq!(status, 408, "{res}");
    assert_eq!(res["error"]["code"], "query_timeout");

    let execution_id = uuid::Uuid::new_v4().to_string();
    let runner = {
        let app = app.clone();
        let token = token.clone();
        let id = id.clone();
        let execution_id = execution_id.clone();
        tokio::spawn(async move {
            exec(
                &app,
                &token,
                &id,
                "SELECT pg_sleep(20)",
                json!({ "execution_id": execution_id }),
            )
            .await
        })
    };
    // Wait until the execution is registered with a backend pid, then cancel.
    // Generous window: connection setup can be slow on loaded CI/dev hosts.
    let mut cancelled = false;
    for _ in 0..300 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (status, body) = send(
            &app,
            api(
                "POST",
                &format!("/api/query/{execution_id}/cancel"),
                None,
                Some(&token),
            ),
        )
        .await;
        if status == 200 && body["cancelled"] == true {
            cancelled = true;
            break;
        }
    }
    assert!(cancelled, "cancel endpoint never succeeded");
    let (status, res) = runner.await.unwrap();
    assert_eq!(status, 409, "{res}");
    assert_eq!(res["error"]["code"], "query_cancelled");

    // Unknown or foreign executions cannot be cancelled.
    let other = register(&app, "b@example.com").await;
    let (status, _) = send(
        &app,
        api(
            "POST",
            &format!("/api/query/{}/cancel", uuid::Uuid::new_v4()),
            None,
            Some(&other),
        ),
    )
    .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn explain_and_explain_analyze(pool: PgPool) {
    let (app, token, id) = setup_with(pool, false, true).await;
    let (status, res) = send(
        &app,
        api(
            "POST",
            "/api/query/explain",
            Some(json!({ "connection_id": id, "query": "SELECT * FROM sales.orders WHERE customer_id IS NOT NULL" })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{res}");
    assert_eq!(res["analyzed"], false);
    assert!(res["plan"]["Node Type"].is_string());
    assert!(res["plan"]["Total Cost"].is_number());

    // ANALYZE requires explicit confirmation.
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/query/explain",
            Some(json!({ "connection_id": id, "query": "SELECT 1", "analyze": true })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 422);

    let (status, res) = send(
        &app,
        api(
            "POST",
            "/api/query/explain",
            Some(json!({ "connection_id": id, "query": "SELECT count(*) FROM sales.orders", "analyze": true, "confirm_analyze": true })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{res}");
    assert_eq!(res["analyzed"], true);
    assert!(res["execution_time_ms"].as_f64().unwrap() >= 0.0);
    assert!(res["plan"]["Actual Rows"].is_number());

    // Even on a read-write connection, ANALYZE of a write is blocked.
    let (status, res) = send(
        &app,
        api(
            "POST",
            "/api/query/explain",
            Some(json!({ "connection_id": id, "query": "DELETE FROM public.departments", "analyze": true, "confirm_analyze": true })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 400, "{res}");
    assert_eq!(res["error"]["details"]["sqlstate"], "25006");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn execution_requires_ownership(pool: PgPool) {
    let (app, _token, id) = setup_with(pool, true, false).await;
    let other = register(&app, "b@example.com").await;
    let (status, _) = exec(&app, &other, &id, "SELECT 1", json!({})).await;
    assert_eq!(status, 404);
}

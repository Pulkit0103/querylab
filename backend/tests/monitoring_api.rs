//! Phase 7 monitoring: sessions, a real blocking lock between two sessions,
//! privilege-limited views, audited cancel/terminate, sizes and statement
//! statistics (against `scripts/sample-db`).

mod common;

use std::time::Duration;

use axum::Router;
use common::{api, register, sample_connection, send};
use serde_json::{json, Value};
use sqlx::{Connection, PgConnection, PgPool};

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

async fn create_connection(app: &Router, token: &str, body: Value) -> String {
    let (status, created) = call(app, token, "POST", "/api/connections", Some(body)).await;
    assert_eq!(status, 201, "{created}");
    created["id"].as_str().unwrap().to_string()
}

fn owner_connection(name: &str, environment: &str) -> Value {
    let mut body = sample_connection(name);
    body["username"] = json!("sample_owner");
    body["password"] = json!("sample_owner");
    body["read_only"] = json!(false);
    body["environment"] = json!(environment);
    body
}

async fn owner_db() -> PgConnection {
    let host = std::env::var("SAMPLE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("SAMPLE_PG_PORT").unwrap_or_else(|_| "5432".into());
    PgConnection::connect(&format!(
        "postgres://sample_owner:sample_owner@{host}:{port}/sampledb"
    ))
    .await
    .expect("connect as sample_owner")
}

async fn pid(db: &mut PgConnection) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(db)
        .await
        .unwrap()
}

fn tree_pids(node: &Value) -> String {
    let children: Vec<String> = node["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(tree_pids)
        .collect();
    if children.is_empty() {
        node["pid"].to_string()
    } else {
        format!("{}[{}]", node["pid"], children.join(","))
    }
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn blocking_sessions_and_signals(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;
    let mut writable_reader = sample_connection("Reader RW");
    writable_reader["read_only"] = json!(false);
    let writable_reader = create_connection(&app, &token, writable_reader).await;
    let owner = create_connection(&app, &token, owner_connection("Owner prod", "production")).await;

    // A holds a row lock; B waits for it.
    let schema = format!("monitor_{}", uuid::Uuid::new_v4().simple());
    let mut setup = owner_db().await;
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; CREATE TABLE {schema}.t (id int PRIMARY KEY, v int); \
         INSERT INTO {schema}.t VALUES (1, 0);"
    ))
    .execute(&mut setup)
    .await
    .unwrap();
    let mut a = owner_db().await;
    let mut b = owner_db().await;
    let (a_pid, b_pid) = (pid(&mut a).await, pid(&mut b).await);
    sqlx::raw_sql(&format!("BEGIN; UPDATE {schema}.t SET v = 1 WHERE id = 1;"))
        .execute(&mut a)
        .await
        .unwrap();
    let blocked_sql = format!("UPDATE {schema}.t SET v = 2 WHERE id = 1");
    let waiter = tokio::spawn(async move {
        let result = sqlx::query(&blocked_sql).execute(&mut b).await;
        result.map(|_| ()).map_err(|e| e.to_string())
    });
    let mut waiting = false;
    for _ in 0..50 {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1 AND wait_event_type = 'Lock'",
        )
        .bind(b_pid)
        .fetch_one(&mut setup)
        .await
        .unwrap();
        if n > 0 {
            waiting = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(waiting, "session B never started waiting");

    // The owner sees the chain with statements.
    let (status, locks) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{owner}/monitor/locks"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{locks}");
    let trees: Vec<String> = locks["tree"]
        .as_array()
        .unwrap()
        .iter()
        .map(tree_pids)
        .collect();
    assert!(trees.contains(&format!("{a_pid}[{b_pid}]")), "{trees:?}");
    assert!(locks["waiting"].as_u64().unwrap() >= 1);
    let b_session = &locks["sessions"][b_pid.to_string()];
    assert!(
        b_session["query"].as_str().unwrap().contains("UPDATE"),
        "{b_session}"
    );
    assert_eq!(b_session["blocked_by"], json!([a_pid]));
    assert_eq!(
        locks["sessions"][a_pid.to_string()]["state"],
        "idle in transaction"
    );
    assert!(locks["locks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["pid"] == b_pid && l["granted"] == false));

    // The least-privileged role sees the same tree without statements.
    let (status, limited) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{reader}/monitor/locks"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{limited}");
    assert_eq!(limited["privileges"]["read_all_stats"], false);
    assert_eq!(limited["privileges"]["superuser"], false);
    assert_eq!(limited["privileges"]["role"], "querylab_reader");
    let trees: Vec<String> = limited["tree"]
        .as_array()
        .unwrap()
        .iter()
        .map(tree_pids)
        .collect();
    assert!(trees.contains(&format!("{a_pid}[{b_pid}]")), "{trees:?}");
    let hidden = &limited["sessions"][b_pid.to_string()];
    assert_eq!(hidden["query_hidden"], true);
    assert_eq!(hidden["query"], Value::Null);

    let (status, activity) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{reader}/monitor/activity"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{activity}");
    let sessions = activity["sessions"].as_array().unwrap();
    assert!(sessions.iter().any(|s| s["is_self"] == true));
    let b_row = sessions
        .iter()
        .find(|s| s["pid"] == b_pid)
        .expect("B listed");
    assert_eq!(b_row["blocked_by"], json!([a_pid]));
    assert_eq!(activity["privileges"]["read_only_connection"], true);

    // Signals: read-only connection, missing privilege, missing confirmation.
    let cancel =
        |conn: &str, pid: i32| format!("/api/connections/{conn}/monitor/backends/{pid}/cancel");
    let (status, res) = call(
        &app,
        &token,
        "POST",
        &cancel(&reader, b_pid),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 403, "{res}");
    let (status, res) = call(
        &app,
        &token,
        "POST",
        &cancel(&writable_reader, b_pid),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 403, "{res}");
    assert!(
        res["error"]["message"]
            .as_str()
            .unwrap()
            .contains("pg_signal_backend"),
        "{res}"
    );
    let (status, res) = call(
        &app,
        &token,
        "POST",
        &cancel(&owner, b_pid),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 422, "{res}");
    let (status, res) = call(
        &app,
        &token,
        "POST",
        &cancel(&owner, 1),
        Some(json!({ "confirm": "Owner prod" })),
    )
    .await;
    assert_eq!(status, 404, "{res}");

    // Cancel B's statement.
    let (status, res) = call(
        &app,
        &token,
        "POST",
        &cancel(&owner, b_pid),
        Some(json!({ "confirm": "Owner prod" })),
    )
    .await;
    assert_eq!(status, 200, "{res}");
    assert_eq!(res["signalled"], true);
    let outcome = tokio::time::timeout(Duration::from_secs(10), waiter)
        .await
        .unwrap()
        .unwrap();
    assert!(outcome.unwrap_err().contains("canceling statement"));

    // Terminate A: its open transaction is gone.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/monitor/backends/{a_pid}/terminate"),
        Some(json!({ "confirm": "Owner prod" })),
    )
    .await;
    assert_eq!(status, 200);
    assert!(sqlx::query("SELECT 1").execute(&mut a).await.is_err());

    // Every attempt that reached the server is audited.
    let (_, history) = call(&app, &token, "GET", "/api/history?kind=admin", None).await;
    let entries = history["items"].as_array().unwrap();
    let sqls: Vec<&str> = entries
        .iter()
        .map(|e| e["sql_content"].as_str().unwrap())
        .collect();
    assert!(
        sqls.iter()
            .any(|s| s.contains(&format!("pg_terminate_backend({a_pid})"))),
        "{sqls:?}"
    );
    assert!(
        sqls.iter()
            .any(|s| s.starts_with(&format!("-- Cancel session {b_pid} (sample_owner"))),
        "{sqls:?}"
    );
    assert!(entries.iter().any(|e| e["status"] == "error"), "{sqls:?}");
    assert!(entries.len() >= 4, "{sqls:?}");

    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut setup)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn sizes_match_postgres_and_statistics_explain_themselves(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;

    let (status, sizes) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{reader}/monitor/sizes?schema=library"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{sizes}");
    assert_eq!(sizes["database"], "sampledb");
    assert!(sizes["database_bytes"].as_i64().unwrap() > 0);
    assert!(sizes["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["schema"] == "sales"));
    let tables = sizes["tables"].as_array().unwrap();
    assert!(tables.iter().all(|t| t["schema"] == "library"));
    let mut db = owner_db().await;
    for t in tables {
        let name = t["name"].as_str().unwrap();
        let expected: i64 = sqlx::query_scalar(
            "SELECT pg_total_relation_size(format('library.%I', $1::text)::regclass)",
        )
        .bind(name)
        .fetch_one(&mut db)
        .await
        .unwrap();
        assert_eq!(t["total_bytes"].as_i64().unwrap(), expected, "{name}");
        assert_eq!(
            t["table_bytes"].as_i64().unwrap() + t["index_bytes"].as_i64().unwrap(),
            expected,
            "{name}"
        );
    }
    assert!(sizes["indexes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["name"] == "books_pkey" && i["primary"] == true));

    // Unavailable statistics are explained, not failed.
    let (status, top) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{reader}/monitor/top-queries?sort=mean"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{top}");
    if top["available"] == false {
        assert!(
            top["reason"]
                .as_str()
                .unwrap()
                .contains("pg_stat_statements"),
            "{top}"
        );
        assert_eq!(top["queries"], json!([]));
    } else {
        assert!(top["queries"].is_array());
    }
    let (status, _) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{reader}/monitor/top-queries?sort=bogus"),
        None,
    )
    .await;
    assert_eq!(status, 400);
    let (status, _) = call(
        &app,
        &token,
        "GET",
        &format!("/api/connections/{reader}/monitor/sizes?schema="),
        None,
    )
    .await;
    assert_eq!(status, 422);
}

//! Phase 9: imports, exports and mock data, end to end against the sample
//! database. Each test works in its own scratch schema.

mod common;

use std::time::Duration;

use axum::{body::Body, http::Request, Router};
use common::{api, register, sample_connection, send, send_full};
use http_body_util::BodyExt;
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

/// Creates a scratch schema with `ddl`, dropped by [`drop_schema`].
async fn scratch_schema(prefix: &str, ddl: &str) -> String {
    let schema = format!("{prefix}_{}", uuid::Uuid::new_v4().simple());
    let mut db = owner_db().await;
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; SET search_path = {schema}; {ddl}; RESET search_path; \
         GRANT USAGE ON SCHEMA {schema} TO querylab_reader; \
         GRANT SELECT ON ALL TABLES IN SCHEMA {schema} TO querylab_reader;"
    ))
    .execute(&mut db)
    .await
    .unwrap();
    schema
}

async fn drop_schema(schema: &str) {
    let mut db = owner_db().await;
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut db)
        .await
        .unwrap();
}

fn upload_request(token: &str, id: &str, offset: usize, bytes: &[u8]) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!("/api/imports/{id}/data?offset={offset}"))
        .header("x-querylab-csrf", "1")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(bytes.to_vec()))
        .unwrap()
}

/// Creates an import, uploads `file` in two chunks and analyzes it.
async fn upload(
    app: &Router,
    token: &str,
    connection: &str,
    name: &str,
    file: &[u8],
) -> (String, Value) {
    let (status, created) = call(
        app,
        token,
        "POST",
        &format!("/api/connections/{connection}/imports"),
        Some(json!({ "file_name": name, "size": file.len() })),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let id = created["import_id"].as_str().unwrap().to_string();
    let half = file.len() / 2;
    let (status, view) = send(app, upload_request(token, &id, 0, &file[..half])).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["status"], "uploading");
    // Out-of-order chunks are refused; the last chunk again is accepted.
    let (status, _) = send(app, upload_request(token, &id, half + 1, &file[half..])).await;
    assert_eq!(status, 409);
    let (status, _) = send(app, upload_request(token, &id, 0, &file[..half])).await;
    assert_eq!(status, 200);
    let (status, view) = send(app, upload_request(token, &id, half, &file[half..])).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["status"], "ready");
    let (status, preview) = call(
        app,
        token,
        "POST",
        &format!("/api/imports/{id}/analyze"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    (id, preview)
}

async fn wait(app: &Router, token: &str, path: &str) -> Value {
    for _ in 0..600 {
        let (status, view) = call(app, token, "GET", path, None).await;
        assert_eq!(status, 200, "{view}");
        if !matches!(
            view["status"].as_str(),
            Some("running" | "ready" | "uploading")
        ) {
            return view;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("job did not finish: {path}");
}

async fn count(schema: &str, sql: &str) -> i64 {
    let mut db = owner_db().await;
    sqlx::query_scalar(&sql.replace("{s}", schema))
        .fetch_one(&mut db)
        .await
        .unwrap()
}

const PEOPLE: &str = "CREATE TABLE people (
    id integer PRIMARY KEY,
    email text NOT NULL UNIQUE,
    age integer CHECK (age >= 0),
    born date,
    note varchar(5)
)";

// Line 1 is the header. Invalid rows: 3 (age), 4 (email NULL), 6 (duplicate
// email), 7 (check), 8 (field count), 9 (note too long).
const CSV: &str = "id,email,age,born,note\n\
1,a@example.com,30,1990-01-01,hi\n\
2,b@example.com,abc,1990-01-02,\n\
3,,40,1990-01-03,\n\
4,d@example.com,50,1990-01-04,\"x\"\n\
5,a@example.com,60,1990-01-05,\n\
6,f@example.com,-1,1990-01-06,\n\
7,g@example.com\n\
8,h@example.com,20,1990-01-08,toolong\n\
9,i@example.com,21,\"1990-01-09\",ok\n";

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn imports_report_rows_in_every_mode(pool: PgPool) {
    let (app, state) = common::app_with_pool(pool);
    let token = register(&app, "imp@example.com").await;
    let owner = create_connection(&app, &token, owner_connection("Owner", "development")).await;
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;
    let schema = scratch_schema("imp", PEOPLE).await;

    // -- Analyze and check --------------------------------------------------
    let (id, preview) = upload(&app, &token, &owner, "people.csv", CSV.as_bytes()).await;
    assert_eq!(preview["format"], "csv");
    assert_eq!(preview["total_rows"], 9);
    assert_eq!(preview["parse_error_count"], 1);
    assert_eq!(preview["parse_errors"][0]["line"], 8);
    let types: Vec<&str> = preview["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["inferred_type"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["integer", "text", "text", "date", "text"]);

    let target = json!({ "schema": schema, "table": "people" });
    let (status, check) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/check"),
        Some(target.clone()),
    )
    .await;
    assert_eq!(status, 200, "{check}");
    assert_eq!(check["mapping"].as_array().unwrap().len(), 5);
    assert_eq!(check["unique_keys"], json!([["id"], ["email"]]));
    assert!(check["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["message"] == "age is integer but the values look like text"));
    let mapping = check["mapping"].clone();

    let run = |mode: &str, duplicates: &str| {
        json!({
            "schema": schema, "table": "people", "mapping": mapping,
            "error_mode": mode, "duplicate_mode": duplicates,
            "conflict_columns": if duplicates == "update" { json!(["id"]) } else { Value::Null },
        })
    };

    // -- All or nothing: nothing is written --------------------------------
    let (status, started) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(run("all_or_nothing", "error")),
    )
    .await;
    assert_eq!(status, 202, "{started}");
    let report = wait(&app, &token, &format!("/api/imports/{id}")).await;
    assert_eq!(report["status"], "failed", "{report}");
    assert_eq!(report["committed"], false);
    assert_eq!(report["rows_inserted"], 0);
    assert_eq!(count(&schema, "SELECT count(*) FROM {s}.people").await, 0);
    // The import has run: it cannot run again.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(run("skip_invalid", "error")),
    )
    .await;
    assert_eq!(status, 409);

    // -- Skip invalid: valid rows are written, each failure has its line -----
    let (id, _) = upload(&app, &token, &owner, "people.csv", CSV.as_bytes()).await;
    let (status, _) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(run("skip_invalid", "error")),
    )
    .await;
    assert_eq!(status, 202);
    let report = wait(&app, &token, &format!("/api/imports/{id}")).await;
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(report["committed"], true);
    assert_eq!(report["rows_inserted"], 3, "{report}");
    assert_eq!(report["rows_failed"], 6, "{report}");
    let mut lines: Vec<(i64, String)> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    lines.sort();
    let at = |line: i64| {
        lines
            .iter()
            .find(|(l, _)| *l == line)
            .map(|(_, m)| m.as_str())
            .unwrap_or_default()
    };
    assert_eq!(
        lines.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
        [3, 4, 6, 7, 8, 9]
    );
    assert!(
        at(3).contains("invalid input syntax for type integer"),
        "{lines:?}"
    );
    assert!(at(4).contains("null value"), "{lines:?}");
    assert!(at(6).contains("duplicate key"), "{lines:?}");
    assert!(at(7).contains("check constraint"), "{lines:?}");
    assert!(at(8).contains("expected 5 fields"), "{lines:?}");
    assert!(at(9).contains("too long"), "{lines:?}");
    assert_eq!(count(&schema, "SELECT count(*) FROM {s}.people").await, 3);

    // -- Duplicates: skip, then update --------------------------------------
    let again = "id,email,age\n1,a@example.com,31\n4,d@example.com,51\n10,j@example.com,22\n";
    let (id, _) = upload(&app, &token, &owner, "again.csv", again.as_bytes()).await;
    let (_, check) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/check"),
        Some(target.clone()),
    )
    .await;
    let mapping = check["mapping"].clone();
    let body = |duplicates: &str| {
        json!({
            "schema": schema, "table": "people", "mapping": mapping,
            "duplicate_mode": duplicates, "error_mode": "all_or_nothing",
            "conflict_columns": if duplicates == "skip" { Value::Null } else { json!(["id"]) },
        })
    };
    call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(body("skip")),
    )
    .await;
    let report = wait(&app, &token, &format!("/api/imports/{id}")).await;
    assert_eq!(
        (
            report["rows_inserted"].as_i64(),
            report["rows_skipped"].as_i64()
        ),
        (Some(1), Some(2)),
        "{report}"
    );

    let (id, _) = upload(&app, &token, &owner, "again.csv", again.as_bytes()).await;
    // The conflict target must be a unique key.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(json!({
            "schema": schema, "table": "people", "mapping": mapping,
            "duplicate_mode": "update", "conflict_columns": ["age"],
        })),
    )
    .await;
    assert_eq!(status, 422);
    call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(body("update")),
    )
    .await;
    let report = wait(&app, &token, &format!("/api/imports/{id}")).await;
    assert_eq!(
        (
            report["rows_inserted"].as_i64(),
            report["rows_updated"].as_i64()
        ),
        (Some(0), Some(3)),
        "{report}"
    );
    assert_eq!(
        count(&schema, "SELECT age::bigint FROM {s}.people WHERE id = 4").await,
        51
    );

    // -- JSON (NDJSON) ------------------------------------------------------
    let ndjson = "{\"id\": 20, \"email\": \"n@example.com\", \"born\": \"2001-02-03\"}\n{\"id\": 21, \"email\": \"o@example.com\", \"age\": 5}\n";
    let (id, preview) = upload(&app, &token, &owner, "people.ndjson", ndjson.as_bytes()).await;
    assert_eq!(preview["format"], "json");
    let (_, check) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/check"),
        Some(target.clone()),
    )
    .await;
    call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(json!({
            "schema": schema, "table": "people", "mapping": check["mapping"],
        })),
    )
    .await;
    let report = wait(&app, &token, &format!("/api/imports/{id}")).await;
    assert_eq!(report["rows_inserted"], 2, "{report}");

    // -- Guards -------------------------------------------------------------
    let (id, _) = upload(&app, &token, &reader, "people.csv", CSV.as_bytes()).await;
    let (status, body) = call(
        &app,
        &token,
        "POST",
        &format!("/api/imports/{id}/run"),
        Some(run("skip_invalid", "error")),
    )
    .await;
    assert_eq!(status, 403, "{body}");
    let other = register(&app, "other@example.com").await;
    let (status, _) = call(&app, &other, "GET", &format!("/api/imports/{id}"), None).await;
    assert_eq!(status, 404);
    let (status, _) = call(&app, &other, "DELETE", &format!("/api/imports/{id}"), None).await;
    assert_eq!(status, 404);
    // Discarding frees the reserved memory.
    let (status, _) = call(&app, &token, "DELETE", &format!("/api/imports/{id}"), None).await;
    assert_eq!(status, 204);
    assert_eq!(state.jobs.reserved_bytes(), 0);
    // Files over the limit are refused before upload.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/imports"),
        Some(json!({ "file_name": "big.csv", "size": 51 * 1024 * 1024 })),
    )
    .await;
    assert_eq!(status, 422);

    // The job log lists the imports.
    let (status, jobs) = call(&app, &token, "GET", "/api/data-jobs?kind=import", None).await;
    assert_eq!(status, 200);
    assert!(jobs.as_array().unwrap().len() >= 5, "{jobs}");

    drop_schema(&schema).await;
}

async fn download(
    app: &Router,
    token: &str,
    connection: &str,
    body: Value,
) -> (u16, String, Vec<u8>, usize) {
    use tower::ServiceExt;
    let res = app
        .clone()
        .oneshot(api(
            "POST",
            &format!("/api/connections/{connection}/exports"),
            Some(body),
            Some(token),
        ))
        .await
        .unwrap();
    let status = res.status().as_u16();
    let content_type = res
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let mut body = res.into_body();
    let mut bytes = Vec::new();
    let mut frames = 0;
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame.unwrap().into_data() {
            frames += 1;
            bytes.extend_from_slice(&data);
        }
    }
    (status, content_type, bytes, frames)
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn exports_stream_queries_and_tables(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "exp@example.com").await;
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;

    let (status, _, body, _) = download(&app, &token, &reader, json!({
        "format": "csv",
        "source": { "type": "query", "sql": "SELECT n, ARRAY[n, n + 1] AS pair, NULL::text AS none FROM generate_series(1, 2) n;" },
    })).await;
    assert_eq!(status, 200);
    assert_eq!(
        String::from_utf8(body).unwrap(),
        "n,pair,none\r\n1,\"{1,2}\",\r\n2,\"{2,3}\",\r\n"
    );

    // SHOW cannot be wrapped; values are decoded instead.
    let (status, _, body, _) = download(&app, &token, &reader, json!({
        "format": "json", "source": { "type": "query", "sql": "SHOW server_encoding", "name": "enc" },
    })).await;
    assert_eq!(status, 200);
    assert_eq!(
        String::from_utf8(body).unwrap(),
        "[\n  {\"server_encoding\": \"UTF8\"}\n]\n"
    );

    let (status, _, body, _) = download(&app, &token, &reader, json!({
        "format": "sql", "source": { "type": "query", "sql": "SELECT 1 AS id, 'x''y' AS name", "name": "things" },
    })).await;
    assert_eq!(status, 200);
    assert!(String::from_utf8(body)
        .unwrap()
        .ends_with("INSERT INTO things (id, name) VALUES (1, 'x''y');\n"));

    // Errors arrive before the download starts.
    let (status, err) = send(
        &app,
        api(
            "POST",
            &format!("/api/connections/{reader}/exports"),
            Some(json!({
                "format": "csv", "source": { "type": "query", "sql": "SELEC 1" },
            })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(err["error"]["code"], "sql_error");

    // Tables (filtered) as Excel.
    let (status, content_type, body, _) = download(&app, &token, &reader, json!({
        "format": "xlsx",
        "source": { "type": "table", "schema": "public", "table": "departments",
                    "filter": { "op": "and", "items": [{ "column": "id", "operator": "<=", "value": 3 }] } },
    })).await;
    assert_eq!(status, 200);
    assert_eq!(
        content_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    assert_eq!(&body[..2], b"PK");

    // Large exports stream in many chunks rather than one buffered body.
    let (status, _, body, frames) = download(&app, &token, &reader, json!({
        "format": "csv", "source": { "type": "query", "sql": "SELECT n, md5(n::text) FROM generate_series(1, 200000) n" },
    })).await;
    assert_eq!(status, 200);
    assert_eq!(body.iter().filter(|&&b| b == b'\n').count(), 200_001);
    assert!(frames > 50, "{frames} frames");

    // The Data Studio route accepts Excel too.
    let (status, headers, _) = send_full(
        &app,
        api(
            "POST",
            &format!("/api/connections/{reader}/data/export"),
            Some(json!({
                "schema": "public", "table": "departments", "format": "xlsx",
            })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 200);
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains("departments.xlsx"));

    let (status, jobs) = call(&app, &token, "GET", "/api/data-jobs?kind=export", None).await;
    assert_eq!(status, 200);
    assert!(jobs.as_array().unwrap().len() >= 5, "{jobs}");

    // One statement only (checked last: it is an error path on the server).
    let (status, _) = send(
        &app,
        api(
            "POST",
            &format!("/api/connections/{reader}/exports"),
            Some(json!({
                "format": "csv", "source": { "type": "query", "sql": "SELECT 1; SELECT 2" },
            })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status.as_u16(), 400);
}

const SHOP: &str = "
    CREATE TABLE departments (id serial PRIMARY KEY, name text NOT NULL UNIQUE);
    CREATE TABLE employees (
        id serial PRIMARY KEY,
        department_id integer NOT NULL REFERENCES departments (id),
        manager_id integer REFERENCES employees (id),
        first_name text NOT NULL,
        email varchar(60) NOT NULL UNIQUE,
        salary numeric(12, 2) NOT NULL CHECK (salary > 0),
        rating integer CHECK (rating BETWEEN 1 AND 5),
        hired_on date NOT NULL
    );
    CREATE TABLE customers (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name text NOT NULL, country char(2) NOT NULL);
    CREATE TABLE orders (
        id bigserial PRIMARY KEY,
        customer_id uuid NOT NULL REFERENCES customers (id),
        sales_rep_id integer REFERENCES employees (id),
        status text NOT NULL CHECK (status IN ('pending', 'paid', 'shipped')),
        total numeric(12, 2) NOT NULL,
        ordered_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE a (id int PRIMARY KEY, b_id int);
    CREATE TABLE b (id int PRIMARY KEY, a_id int REFERENCES a (id));
    ALTER TABLE a ADD FOREIGN KEY (b_id) REFERENCES b (id);
";

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn mock_data_respects_keys_and_checks(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "mock@example.com").await;
    let owner = create_connection(&app, &token, owner_connection("Owner", "development")).await;
    let prod = create_connection(&app, &token, owner_connection("Owner prod", "production")).await;
    let schema = scratch_schema("mock", SHOP).await;
    let table = |name: &str, rows: u64| json!({ "schema": schema, "table": name, "rows": rows });

    // Orders need customers, which have no rows yet.
    let (status, preview) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/mock-data/preview"),
        Some(json!({ "tables": [table("orders", 10)], "seed": 7 })),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(
        preview["missing_parents"],
        json!([format!("{schema}.customers")])
    );

    // Cycles are refused with an explanation.
    let (status, err) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/mock-data/preview"),
        Some(json!({ "tables": [table("a", 5), table("b", 5)] })),
    )
    .await;
    assert_eq!(status, 422);
    assert!(err.to_string().contains("reference each other"), "{err}");

    let request = json!({
        "seed": 42,
        "tables": [table("orders", 3000), table("employees", 300), table("customers", 200), table("departments", 5)],
    });
    let (status, preview) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/mock-data/preview"),
        Some(request.clone()),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    let order: Vec<&str> = preview["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["table"].as_str().unwrap())
        .collect();
    let position = |name: &str| order.iter().position(|t| *t == name).unwrap();
    assert!(position("departments") < position("employees"));
    assert!(position("customers") < position("orders"));
    assert!(position("employees") < position("orders"));
    let employees = &preview["tables"][position("employees")];
    let generator = |name: &str| {
        employees["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()["generator"]
            .clone()
    };
    assert_eq!(generator("id")["kind"], "default");
    assert_eq!(generator("department_id")["kind"], "reference");
    assert_eq!(generator("email")["kind"], "email");
    assert_eq!(
        generator("rating"),
        json!({ "kind": "integer", "min": 1, "max": 5 })
    );
    assert_eq!(employees["sample"].as_array().unwrap().len(), 5);
    let orders = &preview["tables"][position("orders")];
    let status_gen = orders["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "status")
        .unwrap()["generator"]
        .clone();
    assert_eq!(
        status_gen,
        json!({ "kind": "values", "values": ["pending", "paid", "shipped"] })
    );
    assert_eq!(preview["production_blocked"], false);

    // Production is refused.
    let (status, _) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{prod}/mock-data/run"),
        Some(json!({ "tables": [table("departments", 5)], "confirm": "Owner prod" })),
    )
    .await;
    assert_eq!(status, 403);

    // Same seed, same sample.
    let (_, again) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/mock-data/preview"),
        Some(request.clone()),
    )
    .await;
    assert_eq!(
        again["tables"][position("employees")]["sample"],
        employees["sample"]
    );

    let (status, started) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/mock-data/run"),
        Some(request),
    )
    .await;
    assert_eq!(status, 202, "{started}");
    let id = started["id"].as_str().unwrap();
    let report = wait(&app, &token, &format!("/api/data-jobs/{id}")).await;
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(report["committed"], true);
    assert_eq!(report["rows_inserted"], 3505, "{report}");

    // Every foreign key and check holds; unique values are unique.
    assert_eq!(
        count(&schema, "SELECT count(*) FROM {s}.orders").await,
        3000
    );
    assert_eq!(
        count(&schema, "SELECT count(*) FROM {s}.employees").await,
        300
    );
    assert_eq!(count(&schema, "SELECT count(*) FROM {s}.orders o LEFT JOIN {s}.customers c ON c.id = o.customer_id WHERE c.id IS NULL").await, 0);
    assert_eq!(
        count(
            &schema,
            "SELECT count(DISTINCT department_id) FROM {s}.employees"
        )
        .await,
        5
    );
    assert!(
        count(
            &schema,
            "SELECT count(*) FROM {s}.employees WHERE manager_id IS NOT NULL"
        )
        .await
            > 0
    );
    assert!(
        count(
            &schema,
            "SELECT count(*) FROM {s}.orders WHERE sales_rep_id IS NOT NULL"
        )
        .await
            > 2500
    );
    assert_eq!(
        count(
            &schema,
            "SELECT count(*) FROM {s}.employees WHERE rating NOT BETWEEN 1 AND 5 OR salary <= 0"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &schema,
            "SELECT count(*) FROM {s}.customers WHERE length(country) <> 2"
        )
        .await,
        0
    );

    // A second run reuses existing rows and keeps unique values unique.
    let (status, started) = call(
        &app,
        &token,
        "POST",
        &format!("/api/connections/{owner}/mock-data/run"),
        Some(json!({ "tables": [table("departments", 5), table("employees", 50)] })),
    )
    .await;
    assert_eq!(status, 202, "{started}");
    let report = wait(
        &app,
        &token,
        &format!("/api/data-jobs/{}", started["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(
        count(&schema, "SELECT count(*) FROM {s}.employees").await,
        350
    );

    // History records the run.
    let (_, history) = call(&app, &token, "GET", "/api/history?kind=mock_data", None).await;
    assert!(
        history.to_string().contains("Mock data (seed 42)"),
        "{history}"
    );

    drop_schema(&schema).await;
}

//! Data Studio: browse, preview, apply and export (against `scripts/sample-db`).
//!
//! Write tests run as `sample_owner` inside a scratch schema that is dropped
//! afterwards, so the shared fixtures are never modified.

mod common;

use axum::{body::Body, http::Request, Router};
use common::{api, register, sample_connection, send};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{Connection, PgConnection, PgPool};

async fn create_connection(app: &Router, token: &str, body: Value) -> String {
    let (status, created) = send(
        app,
        api("POST", "/api/connections", Some(body), Some(token)),
    )
    .await;
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

async fn post(app: &Router, token: &str, path: &str, body: Value) -> (u16, Value) {
    let (status, json) = send(app, api("POST", path, Some(body), Some(token))).await;
    (status.as_u16(), json)
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

/// A scratch schema with an editable table, a keyless table and a table
/// whose identifiers need quoting.
async fn scratch_schema(db: &mut PgConnection) -> String {
    let schema = format!("studio_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        r#"CREATE SCHEMA {schema};
           CREATE TABLE {schema}.people (
               id serial PRIMARY KEY,
               name text NOT NULL,
               age integer CHECK (age >= 0),
               note text,
               profile jsonb
           );
           INSERT INTO {schema}.people (name, age, note) VALUES ('Ada', 36, 'first'), ('Grace', 45, NULL), ('Linus', 28, 'x');
           CREATE TABLE {schema}.log (msg text);
           INSERT INTO {schema}.log VALUES ('hello');
           CREATE TABLE {schema}."we""ird; table" ("key ""id""" int PRIMARY KEY, "x"" OR 1=1" text);
           INSERT INTO {schema}."we""ird; table" VALUES (1, 'one');"#
    ))
    .execute(&mut *db)
    .await
    .expect("create scratch schema");
    schema
}

async fn drop_schema(db: &mut PgConnection, schema: &str) {
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(db)
        .await
        .unwrap();
}

async fn raw(app: &Router, token: &str, path: &str, body: Value) -> (u16, String, String) {
    use tower::ServiceExt;
    let req: Request<Body> = api("POST", path, Some(body), Some(token));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let disposition = res
        .headers()
        .get("content-disposition")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        disposition,
        String::from_utf8(bytes.to_vec()).unwrap(),
    )
}

fn column(rows: &Value, index: usize) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| r[index].as_str().unwrap_or("NULL").to_string())
        .collect()
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn browse_paginates_filters_and_sorts(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let id = create_connection(&app, &token, sample_connection("Sample")).await;
    let browse = format!("/api/connections/{id}/data/browse");

    // First page: keyset over the primary key, exact count, read-only.
    let (status, page) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "page_size": 70 }),
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["pagination"], "keyset");
    assert_eq!(page["total_rows"], 200);
    assert_eq!(page["total_exact"], true);
    assert_eq!(page["key_columns"], json!(["id"]));
    assert_eq!(page["key_kind"], "primary_key");
    assert_eq!(page["editable"], false);
    assert!(page["read_only_reason"]
        .as_str()
        .unwrap()
        .contains("read-only"));
    let salary = page["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "salary")
        .unwrap();
    assert_eq!(salary["kind"], "number");
    assert_eq!(salary["data_type"], "numeric(12,2)");
    assert_eq!(
        page["sql"],
        "SELECT * FROM public.employees ORDER BY id LIMIT 70"
    );

    // Walking the cursors visits every row exactly once, in order.
    let mut ids: Vec<i64> = Vec::new();
    let mut page = page;
    loop {
        ids.extend(
            column(&page["rows"], 0)
                .iter()
                .map(|v| v.parse::<i64>().unwrap()),
        );
        let Some(cursor) = page["next_cursor"].as_str().map(str::to_string) else {
            break;
        };
        assert!(page["total_rows"].is_null() || ids.len() == 70);
        let (status, next) = post(
            &app,
            &token,
            &browse,
            json!({ "schema": "public", "table": "employees", "page_size": 70, "cursor": cursor }),
        )
        .await;
        assert_eq!(status, 200, "{next}");
        page = next;
    }
    assert_eq!(ids, (1..=200).collect::<Vec<i64>>());

    // Structured filter: same rows as the equivalent hand-written SQL.
    let filter = json!({ "op": "and", "items": [
        { "column": "salary", "operator": ">", "value": 100000 },
        { "column": "is_active", "operator": "=", "value": true },
        { "op": "or", "items": [
            { "column": "department_id", "operator": "in", "value": [1, 2] },
            { "column": "hired_on", "operator": "between", "value": ["2015-01-01", "2017-12-31"] }
        ]}
    ]});
    let (status, filtered) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "filter": filter, "page_size": 1000 }),
    )
    .await;
    assert_eq!(status, 200, "{filtered}");
    assert_eq!(
        filtered["sql"],
        "SELECT * FROM public.employees WHERE salary > 100000 AND is_active = true \
         AND (department_id IN (1, 2) OR hired_on BETWEEN '2015-01-01' AND '2017-12-31') \
         ORDER BY id LIMIT 1000"
    );
    let (_, expected) = post(
        &app,
        &token,
        "/api/query/execute",
        json!({ "connection_id": id, "query":
            "SELECT id::text AS id_text FROM public.employees WHERE salary > 100000 AND is_active \
             AND (department_id IN (1, 2) OR hired_on BETWEEN '2015-01-01' AND '2017-12-31') ORDER BY id" }),
    )
    .await;
    assert_eq!(column(&filtered["rows"], 0), column(&expected["rows"], 0));
    assert_eq!(
        filtered["total_rows"].as_u64().unwrap() as usize,
        expected["rows"].as_array().unwrap().len()
    );

    // Sorting by another column switches to offset pages with a stable tiebreak.
    let sort = json!([{ "column": "last_name", "direction": "desc" }]);
    let (_, first) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "sort": sort, "page_size": 30 }),
    )
    .await;
    assert_eq!(first["pagination"], "offset");
    let (_, second) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "sort": sort, "page_size": 30,
                "cursor": first["next_cursor"] }),
    )
    .await;
    let (_, expected) = post(
        &app,
        &token,
        "/api/query/execute",
        json!({ "connection_id": id, "query":
            "SELECT id::text AS id_text FROM public.employees ORDER BY last_name DESC, id OFFSET 30 LIMIT 30" }),
    )
    .await;
    assert_eq!(column(&second["rows"], 0), column(&expected["rows"], 0));

    // Quick search across columns; LIKE wildcards in the term are literal.
    let (_, found) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "search": "employee17@" }),
    )
    .await;
    assert_eq!(found["total_rows"], 1, "{found}");
    let (_, none) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "search": "employee_7@" }),
    )
    .await;
    assert_eq!(none["total_rows"], 0);

    // Views have no row key: offset pages, never editable.
    let (status, view) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "sales", "table": "revenue_by_country" }),
    )
    .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["pagination"], "offset");
    assert_eq!(view["kind"], "view");
    assert!(view["key_columns"].as_array().unwrap().is_empty());

    // Catalog-aware column details.
    let (_, products) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "inventory", "table": "products" }),
    )
    .await;
    let col = |name: &str| {
        products["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(col("status")["kind"], "enum");
    assert_eq!(
        col("status")["enum_values"],
        json!(["in_stock", "low", "out_of_stock"])
    );
    assert_eq!(
        col("quantity")["type_name"],
        "int4",
        "domains resolve to their base type"
    );
    assert_eq!(col("price_with_tax")["editable"], false);
    assert_eq!(col("id")["identity"], "always");
    assert_eq!(col("id")["editable"], false);

    // Errors: unknown column, unknown table, bad cursor.
    let bad_filter = json!({ "items": [{ "column": "nope", "operator": "=", "value": 1 }] });
    let (status, _) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "filter": bad_filter }),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "nope" }),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "cursor": "garbage" }),
    )
    .await;
    assert_eq!(status, 422);
    // A bad value is a SQL error from PostgreSQL's input function.
    let (status, err) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees",
                "filter": { "items": [{ "column": "salary", "operator": ">", "value": "lots" }] } }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(err["error"]["details"]["sqlstate"], "22P02");

    // Hostile values are bound, never executed.
    let (status, hostile) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees", "filter": { "items": [
            { "column": "first_name", "operator": "=", "value": "x'; DROP TABLE public.employees; --" }
        ]}}),
    )
    .await;
    assert_eq!(status, 200, "{hostile}");
    assert_eq!(hostile["total_rows"], 0);
    let (_, still) = post(
        &app,
        &token,
        &browse,
        json!({ "schema": "public", "table": "employees" }),
    )
    .await;
    assert_eq!(still["total_rows"], 200);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn apply_is_atomic_and_detects_conflicts(pool: PgPool) {
    let mut db = owner_db().await;
    let schema = scratch_schema(&mut db).await;
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let id = create_connection(&app, &token, owner_connection("Owner", "development")).await;
    let base = format!("/api/connections/{id}/data");
    let people =
        |changes: Value| json!({ "schema": schema, "table": "people", "changes": changes });
    async fn ages(db: &mut PgConnection, schema: &str) -> String {
        let sql = format!(
            "SELECT string_agg(name || '=' || coalesce(age::text, 'null'), ',' ORDER BY id) FROM {schema}.people"
        );
        sqlx::query_scalar::<_, String>(&sql)
            .fetch_one(db)
            .await
            .unwrap()
    }

    // Browse marks the table editable.
    let (_, page) = post(
        &app,
        &token,
        &format!("{base}/browse"),
        json!({ "schema": schema, "table": "people" }),
    )
    .await;
    assert_eq!(page["editable"], true, "{page}");
    assert_eq!(page["can_update"], true);

    // Preview renders the exact statements, in execution order.
    let changes = json!({
        "inserts": [{ "values": { "name": "Barbara", "age": 30, "profile": { "langs": ["CLU"] } } }],
        "updates": [{ "key": { "id": "1" }, "values": { "age": 37, "note": null }, "original": { "age": "36", "note": "first" } }],
        "deletes": [{ "key": { "id": 3 } }]
    });
    let (status, preview) = post(
        &app,
        &token,
        &format!("{base}/preview"),
        json!({ "schema": schema, "table": "people", "changes": changes }),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    let sqls: Vec<&str> = preview["statements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["sql"].as_str().unwrap())
        .collect();
    assert_eq!(sqls, [
        format!("DELETE FROM {schema}.people WHERE id = 3;"),
        format!("UPDATE {schema}.people SET age = 37, note = NULL WHERE id = 1 AND CAST(age AS text) IS NOT DISTINCT FROM '36' AND CAST(note AS text) IS NOT DISTINCT FROM 'first';"),
        format!("INSERT INTO {schema}.people (name, age, profile) VALUES ('Barbara', 30, '{{\"langs\":[\"CLU\"]}}');"),
    ]);

    // Apply: all three statements commit together and return stored rows.
    let (status, applied) = post(&app, &token, &format!("{base}/apply"), people(changes)).await;
    assert_eq!(status, 200, "{applied}");
    assert_eq!(applied["committed"], true, "{applied}");
    assert_eq!(applied["rows_affected"], 3);
    let inserted = applied["applied"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "insert")
        .unwrap();
    assert_eq!(inserted["row"][0], "4", "serial id is returned");
    assert_eq!(inserted["row"][4], "{\"langs\": [\"CLU\"]}");
    assert_eq!(ages(&mut db, &schema).await, "Ada=37,Grace=45,Barbara=30");

    // Stale write: the original value no longer matches. The delete that ran
    // first is rolled back with it.
    let (_, stale) = post(
        &app,
        &token,
        &format!("{base}/apply"),
        people(json!({
            "deletes": [{ "key": { "id": 2 } }],
            "updates": [{ "key": { "id": 1 }, "values": { "age": 99 }, "original": { "age": "36" } }]
        })),
    )
    .await;
    assert_eq!(stale["committed"], false, "{stale}");
    assert_eq!(stale["failure"]["conflict"], true);
    assert_eq!(stale["failure"]["kind"], "update");
    assert_eq!(stale["failure"]["index"], 0);
    assert!(stale["failure"]["message"]
        .as_str()
        .unwrap()
        .contains("changed or deleted"));
    assert!(stale["applied"].as_array().unwrap().is_empty());
    assert_eq!(ages(&mut db, &schema).await, "Ada=37,Grace=45,Barbara=30");

    // Missing row.
    let (_, missing) = post(
        &app,
        &token,
        &format!("{base}/apply"),
        people(json!({ "deletes": [{ "key": { "id": 999 } }] })),
    )
    .await;
    assert_eq!(missing["committed"], false);
    assert!(missing["failure"]["message"]
        .as_str()
        .unwrap()
        .contains("no longer exists"));

    // Constraint violation rolls back the update that preceded it.
    let (_, violation) = post(
        &app,
        &token,
        &format!("{base}/apply"),
        people(json!({
            "updates": [{ "key": { "id": 2 }, "values": { "age": 46 } }],
            "inserts": [{ "values": { "name": "Bad", "age": -1 } }]
        })),
    )
    .await;
    assert_eq!(violation["committed"], false, "{violation}");
    assert_eq!(violation["failure"]["conflict"], false);
    assert_eq!(violation["failure"]["kind"], "insert");
    assert_eq!(violation["failure"]["sql_error"]["sqlstate"], "23514");
    assert_eq!(ages(&mut db, &schema).await, "Ada=37,Grace=45,Barbara=30");

    // Validation and read-only relations.
    let (status, _) = post(
        &app,
        &token,
        &format!("{base}/apply"),
        people(json!({ "updates": [{ "key": { "name": "Ada" }, "values": { "age": 1 } }] })),
    )
    .await;
    assert_eq!(status, 422, "the key must be the primary key");
    let (status, keyless) = post(
        &app,
        &token,
        &format!("{base}/apply"),
        json!({ "schema": schema, "table": "log", "changes": { "inserts": [{ "values": { "msg": "x" } }] } }),
    )
    .await;
    assert_eq!(status, 403, "{keyless}");

    // Identifiers that need quoting work end to end.
    let weird = json!({ "schema": schema, "table": "we\"ird; table" });
    let (status, page) = post(&app, &token, &format!("{base}/browse"), weird.clone()).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["key_columns"], json!(["key \"id\""]));
    let mut req = weird.clone();
    req["changes"] =
        json!({ "updates": [{ "key": { "key \"id\"": 1 }, "values": { "x\" OR 1=1": "two" } }] });
    let (status, done) = post(&app, &token, &format!("{base}/apply"), req).await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["committed"], true, "{done}");
    let value: String = sqlx::query_scalar(&format!(
        "SELECT \"x\"\" OR 1=1\" FROM {schema}.\"we\"\"ird; table\""
    ))
    .fetch_one(&mut db)
    .await
    .unwrap();
    assert_eq!(value, "two");

    // Applies are recorded in history.
    let (_, history) = send(&app, api("GET", "/api/history", None, Some(&token))).await;
    let entries = history["items"].as_array().unwrap();
    assert!(entries.iter().any(|h| h["sql_content"]
        .as_str()
        .unwrap()
        .starts_with("-- Data Studio: 3 change(s)")
        && h["status"] == "success"));
    assert!(entries.iter().any(|h| h["status"] == "error"
        && h["error_message"]
            .as_str()
            .unwrap()
            .contains("check constraint")));

    drop_schema(&mut db, &schema).await;
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn writes_need_a_writable_connection_and_confirmation(pool: PgPool) {
    let mut db = owner_db().await;
    let schema = scratch_schema(&mut db).await;
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let change = json!({ "schema": schema, "table": "people",
                         "changes": { "updates": [{ "key": { "id": 1 }, "values": { "age": 50 } }] } });

    // Read-only connection.
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;
    let (status, body) = post(
        &app,
        &token,
        &format!("/api/connections/{reader}/data/apply"),
        change.clone(),
    )
    .await;
    assert_eq!(status, 403, "{body}");

    // Production needs the connection name typed back.
    let prod = create_connection(&app, &token, owner_connection("Prod Owner", "production")).await;
    let path = format!("/api/connections/{prod}/data/apply");
    let (status, _) = post(&app, &token, &path, change.clone()).await;
    assert_eq!(status, 422);
    let mut wrong = change.clone();
    wrong["confirm"] = json!("prod owner");
    assert_eq!(post(&app, &token, &path, wrong).await.0, 422);
    let mut confirmed = change.clone();
    confirmed["confirm"] = json!("Prod Owner");
    let (status, body) = post(&app, &token, &path, confirmed).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["committed"], true);

    // Staging needs any confirmation.
    let staging = create_connection(&app, &token, owner_connection("Stage", "staging")).await;
    let path = format!("/api/connections/{staging}/data/apply");
    assert_eq!(post(&app, &token, &path, change.clone()).await.0, 422);
    let mut confirmed = change.clone();
    confirmed["confirm"] = json!("confirm");
    assert_eq!(post(&app, &token, &path, confirmed).await.0, 200);

    // Other users cannot reach any Data Studio endpoint.
    let other = register(&app, "b@example.com").await;
    for endpoint in ["browse", "preview", "apply", "export"] {
        let mut body = change.clone();
        body["format"] = json!("csv");
        body["confirm"] = json!("Prod Owner");
        let (status, _) = post(
            &app,
            &other,
            &format!("/api/connections/{prod}/data/{endpoint}"),
            body,
        )
        .await;
        assert_eq!(status, 404, "{endpoint}");
    }

    drop_schema(&mut db, &schema).await;
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn exports_stream_csv_json_and_sql(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let id = create_connection(&app, &token, sample_connection("Sample")).await;
    let path = format!("/api/connections/{id}/data/export");
    let request = |format: &str| {
        json!({
            "schema": "inventory", "table": "products", "format": format,
            "columns": ["id", "name", "price", "status"],
            "filter": { "items": [{ "column": "price", "operator": "<", "value": 100 }] },
            "sort": [{ "column": "price" }]
        })
    };

    let (status, disposition, csv) = raw(&app, &token, &path, request("csv")).await;
    assert_eq!(status, 200, "{csv}");
    assert_eq!(disposition, "attachment; filename=\"products.csv\"");
    assert_eq!(
        csv,
        "id,name,price,status\r\n2,Mouse,19.50,low\r\n1,Keyboard,49.00,in_stock\r\n"
    );

    let (status, _, body) = raw(&app, &token, &path, request("json")).await;
    assert_eq!(status, 200, "{body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed,
        json!([
            { "id": 2, "name": "Mouse", "price": 19.5, "status": "low" },
            { "id": 1, "name": "Keyboard", "price": 49.0, "status": "in_stock" }
        ])
    );

    let (status, _, sql) = raw(&app, &token, &path, request("sql")).await;
    assert_eq!(status, 200, "{sql}");
    assert!(sql.contains(
        "INSERT INTO inventory.products (id, name, price, status) OVERRIDING SYSTEM VALUE VALUES (2, 'Mouse', 19.50, 'low');"
    ), "{sql}");

    // Selected rows only, all columns (generated columns skipped for SQL).
    let (status, _, sql) = raw(
        &app,
        &token,
        &path,
        json!({ "schema": "inventory", "table": "products", "format": "sql", "keys": [{ "id": 3 }] }),
    )
    .await;
    assert_eq!(status, 200, "{sql}");
    assert_eq!(sql.matches("INSERT INTO").count(), 1);
    assert!(
        sql.contains("'Monitor'") && !sql.contains("price_with_tax"),
        "{sql}"
    );

    // Validation errors are JSON, before any streaming starts.
    let (status, body) = post(
        &app,
        &token,
        &path,
        json!({ "schema": "inventory", "table": "products", "format": "csv", "columns": ["nope"] }),
    )
    .await;
    assert_eq!(status, 422, "{body}");
}

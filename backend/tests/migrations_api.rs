//! Phase 8: schema diff and migrations, end to end against the sample
//! database. Source and target are two scratch schemas in `sampledb`.

mod common;

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

const SOURCE: &str = r#"
    CREATE TYPE mood AS ENUM ('sad', 'ok', 'happy');
    CREATE DOMAIN positive AS integer CHECK (VALUE >= 0);
    CREATE TABLE authors (
        id serial PRIMARY KEY,
        name varchar(100) NOT NULL,
        mood mood DEFAULT 'happy',
        rating positive
    );
    CREATE TABLE books (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        author_id integer NOT NULL REFERENCES authors (id),
        title text NOT NULL,
        amount numeric(10, 2) CHECK (amount >= 0)
    );
    CREATE INDEX books_author_idx ON books (author_id);
    CREATE FUNCTION touch() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
    CREATE TRIGGER books_touch BEFORE UPDATE ON books FOR EACH ROW EXECUTE FUNCTION touch();
    CREATE VIEW book_titles AS SELECT b.title, a.name FROM books b JOIN authors a ON a.id = b.author_id;
    CREATE FUNCTION book_count() RETURNS bigint LANGUAGE sql STABLE AS $$ SELECT count(*) FROM books $$;
"#;

const TARGET: &str = r#"
    CREATE TYPE mood AS ENUM ('sad', 'happy');
    CREATE TABLE authors (
        id serial PRIMARY KEY,
        name varchar(50) NOT NULL,
        legacy text
    );
    CREATE TABLE old_stuff (id int);
    CREATE FUNCTION book_count() RETURNS bigint LANGUAGE sql STABLE AS $$ SELECT 0::bigint $$;
"#;

fn change<'a>(diff: &'a Value, object: &str) -> &'a Value {
    diff["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["object"] == object)
        .unwrap_or_else(|| panic!("no change for {object}: {diff:#}"))
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn diff_apply_and_manage_migrations(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let owner = create_connection(&app, &token, owner_connection("Owner", "development")).await;
    let prod = create_connection(&app, &token, owner_connection("Owner prod", "production")).await;
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let (src, tgt) = (format!("diff_src_{suffix}"), format!("diff_tgt_{suffix}"));
    let mut db = owner_db().await;
    for (schema, ddl) in [(&src, SOURCE), (&tgt, TARGET)] {
        sqlx::raw_sql(&format!(
            "CREATE SCHEMA {schema}; SET search_path = {schema}; {ddl}; RESET search_path;"
        ))
        .execute(&mut db)
        .await
        .unwrap();
    }

    // -- Diff -------------------------------------------------------------------
    let diff_body = json!({
        "source": { "connection_id": owner, "schema": src },
        "target": { "connection_id": reader, "schema": tgt },
    });
    let (status, diff) = call(
        &app,
        &token,
        "POST",
        "/api/schema-diff",
        Some(diff_body.clone()),
    )
    .await;
    assert_eq!(status, 200, "{diff}");
    assert_eq!(
        diff["preamble"],
        format!("SET LOCAL search_path = {tgt};\nSET LOCAL check_function_bodies = off;")
    );
    let kinds = |object: &str| {
        let c = change(&diff, object);
        (
            c["object_type"].as_str().unwrap().to_string(),
            c["action"].as_str().unwrap().to_string(),
            c["destructive"].as_bool().unwrap(),
        )
    };
    assert_eq!(kinds("mood"), ("type".into(), "alter".into(), false));
    assert_eq!(kinds("positive"), ("type".into(), "create".into(), false));
    assert_eq!(
        kinds("authors.name"),
        ("column".into(), "alter".into(), false)
    );
    assert_eq!(
        kinds("authors.mood"),
        ("column".into(), "create".into(), false)
    );
    assert_eq!(
        kinds("authors.legacy"),
        ("column".into(), "drop".into(), true)
    );
    assert_eq!(kinds("old_stuff"), ("table".into(), "drop".into(), true));
    assert_eq!(kinds("books"), ("table".into(), "create".into(), false));
    assert_eq!(
        kinds("books.books_author_id_fkey"),
        ("constraint".into(), "create".into(), false)
    );
    assert_eq!(
        kinds("books.books_amount_check"),
        ("constraint".into(), "create".into(), false)
    );
    assert_eq!(
        kinds("books.books_author_idx"),
        ("index".into(), "create".into(), false)
    );
    assert_eq!(
        kinds("book_titles"),
        ("view".into(), "create".into(), false)
    );
    assert_eq!(
        kinds("touch()"),
        ("function".into(), "create".into(), false)
    );
    assert_eq!(
        kinds("book_count()"),
        ("function".into(), "alter".into(), false)
    );
    assert_eq!(
        kinds("books.books_touch"),
        ("trigger".into(), "create".into(), false)
    );
    assert!(change(&diff, "mood")["sql"]
        .as_str()
        .unwrap()
        .contains("ADD VALUE IF NOT EXISTS 'ok' AFTER 'sad'"));
    // Phases are ordered: drops first, then creates by dependency.
    let phases: Vec<&str> = diff["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["phase"].as_str().unwrap())
        .collect();
    assert_eq!(phases.first(), Some(&"drop_column"));
    assert_eq!(phases.last(), Some(&"create_trigger"));

    let script = format!(
        "{}\n\n{}",
        diff["preamble"].as_str().unwrap(),
        diff["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["sql"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    // -- Project and script -------------------------------------------------------
    let project_name = format!("app-{suffix}");
    let (status, project) = call(
        &app,
        &token,
        "POST",
        "/api/migration-projects",
        Some(json!({ "name": project_name })),
    )
    .await;
    assert_eq!(status, 201, "{project}");
    let pid = project["id"].as_str().unwrap().to_string();
    assert_eq!(
        call(
            &app,
            &token,
            "POST",
            "/api/migration-projects",
            Some(json!({ "name": project_name }))
        )
        .await
        .0,
        409
    );

    let create = |version: i64, name: &str, up: &str, down: Option<&str>| json!({ "version": version, "name": name, "up_sql": up, "down_sql": down });
    let migrations = format!("/api/migration-projects/{pid}/migrations");
    let (status, m1) = call(
        &app,
        &token,
        "POST",
        &migrations,
        Some(create(1, "sync with source", &script, None)),
    )
    .await;
    assert_eq!(status, 201, "{m1}");
    assert_eq!(m1["checksum"].as_str().unwrap().len(), 64);
    let m1 = m1["id"].as_str().unwrap().to_string();
    let (status, err) = call(
        &app,
        &token,
        "POST",
        &migrations,
        Some(create(9, "bad", "BEGIN; select 1; COMMIT;", None)),
    )
    .await;
    assert_eq!(status, 422, "{err}");
    assert_eq!(
        call(
            &app,
            &token,
            "POST",
            &migrations,
            Some(create(1, "dup", "select 1", None))
        )
        .await
        .0,
        409
    );

    let apply = |id: &str, connection: &str, extra: Value| {
        let mut body = json!({ "connection_id": connection });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        (format!("/api/migrations/{id}/apply"), body)
    };

    // Guardrails.
    let (path, body) = apply(&m1, &reader, json!({}));
    assert_eq!(call(&app, &token, "POST", &path, Some(body)).await.0, 403);
    let (path, body) = apply(&m1, &prod, json!({}));
    assert_eq!(call(&app, &token, "POST", &path, Some(body)).await.0, 422);

    // Dry run: succeeds and changes nothing.
    let (path, body) = apply(&m1, &owner, json!({ "dry_run": true }));
    let (status, dry) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 200, "{dry}");
    assert_eq!(dry["status"], "success", "{dry}");
    assert_eq!(dry["run_id"], Value::Null);
    let still: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(format!("{tgt}.old_stuff"))
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert!(still, "dry run must roll back");

    // Apply, then the schemas match.
    let (path, body) = apply(&m1, &owner, json!({}));
    let (status, run) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(run["status"], "success", "{run}");
    let (_, again) = call(&app, &token, "POST", "/api/schema-diff", Some(diff_body)).await;
    assert_eq!(again["changes"], json!([]), "{again:#}");

    let status_path =
        |conn: &str| format!("/api/migration-projects/{pid}/status?connection_id={conn}");
    let (_, st) = call(&app, &token, "GET", &status_path(&owner), None).await;
    assert_eq!(st["tracking_table"], true);
    assert_eq!(st["migrations"][0]["state"], "applied");
    assert!(st["migrations"][0]["applied_by"]
        .as_str()
        .unwrap()
        .contains("a@example.com"));

    // Applied scripts are locked.
    let (status, _) = call(
        &app,
        &token,
        "PUT",
        &format!("/api/migrations/{m1}"),
        Some(json!({ "name": "renamed" })),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(
        call(
            &app,
            &token,
            "DELETE",
            &format!("/api/migrations/{m1}"),
            None
        )
        .await
        .0,
        409
    );

    // Ordering and rollback.
    let extra = format!("{tgt}.extra");
    let (_, m2) = call(
        &app,
        &token,
        "POST",
        &migrations,
        Some(create(
            2,
            "extra table",
            &format!("CREATE TABLE {extra} (id int);"),
            Some(&format!("DROP TABLE {extra};")),
        )),
    )
    .await;
    let m2 = m2["id"].as_str().unwrap().to_string();
    let (_, m3) = call(
        &app,
        &token,
        "POST",
        &migrations,
        Some(create(3, "broken", "SELECT 1/0;", None)),
    )
    .await;
    let m3 = m3["id"].as_str().unwrap().to_string();
    let (path, body) = apply(&m3, &owner, json!({}));
    let (status, res) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 409, "{res}");
    assert!(res["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Apply version 2 first"));
    let (path, body) = apply(&m2, &owner, json!({}));
    assert_eq!(
        call(&app, &token, "POST", &path, Some(body)).await.1["status"],
        "success"
    );
    let rollback = |id: &str| {
        (
            format!("/api/migrations/{id}/rollback"),
            json!({ "connection_id": owner }),
        )
    };
    let (path, body) = rollback(&m1);
    let (status, res) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 409, "{res}");
    let (path, body) = rollback(&m2);
    let (status, res) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(
        (status, res["status"].as_str()),
        (200, Some("success")),
        "{res}"
    );
    let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(&extra)
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert!(!exists);

    // A failing script is rolled back and reported.
    let (path, body) = apply(&m2, &owner, json!({}));
    call(&app, &token, "POST", &path, Some(body)).await;
    let (path, body) = apply(&m3, &owner, json!({}));
    let (status, res) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 200);
    assert_eq!(res["status"], "error");
    assert!(
        res["error"]["message"]
            .as_str()
            .unwrap()
            .contains("division by zero"),
        "{res}"
    );
    let (_, st) = call(&app, &token, "GET", &status_path(&owner), None).await;
    assert_eq!(st["migrations"][2]["state"], "pending");

    // Concurrent applies: the advisory lock makes the second fail fast.
    let mut holder = owner_db().await;
    sqlx::query("SELECT pg_advisory_lock(hashtext('querylab_migrations'))")
        .execute(&mut holder)
        .await
        .unwrap();
    let (path, body) = apply(&m3, &owner, json!({}));
    let (status, res) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 409, "{res}");
    assert!(res["error"]["message"]
        .as_str()
        .unwrap()
        .contains("another migration"));
    sqlx::query("SELECT pg_advisory_unlock(hashtext('querylab_migrations'))")
        .execute(&mut holder)
        .await
        .unwrap();

    // Checksum drift blocks further applies.
    sqlx::query("UPDATE querylab.schema_migrations SET checksum = 'edited' WHERE project = $1 AND version = 1")
        .bind(&project_name)
        .execute(&mut db)
        .await
        .unwrap();
    let (_, st) = call(&app, &token, "GET", &status_path(&owner), None).await;
    assert_eq!(st["migrations"][0]["state"], "drift");
    let (path, body) = apply(&m3, &owner, json!({}));
    let (status, res) = call(&app, &token, "POST", &path, Some(body)).await;
    assert_eq!(status, 409);
    assert!(res["error"]["message"]
        .as_str()
        .unwrap()
        .contains("checksum mismatch"));

    // History of runs and audit entries.
    let (_, runs) = call(
        &app,
        &token,
        "GET",
        &format!("/api/migration-runs?project_id={pid}"),
        None,
    )
    .await;
    let runs = runs.as_array().unwrap();
    assert_eq!(runs.len(), 5, "{runs:?}");
    assert!(runs
        .iter()
        .any(|r| r["direction"] == "down" && r["status"] == "success"));
    assert!(runs
        .iter()
        .any(|r| r["status"] == "error" && r["version"] == 3));
    let (_, history) = call(&app, &token, "GET", "/api/history?kind=migration", None).await;
    assert_eq!(history["items"].as_array().unwrap().len(), 5);

    // Other users see nothing.
    let mallory = register(&app, "m@example.com").await;
    assert_eq!(call(&app, &mallory, "GET", &migrations, None).await.0, 404);
    let (path, body) = apply(&m1, &owner, json!({}));
    assert_eq!(call(&app, &mallory, "POST", &path, Some(body)).await.0, 404);
    let (status, _) = call(
        &app,
        &mallory,
        "POST",
        "/api/schema-diff",
        Some(json!({
            "source": { "connection_id": owner, "schema": src },
            "target": { "connection_id": owner, "schema": tgt },
        })),
    )
    .await;
    assert_eq!(status, 404);

    sqlx::raw_sql(&format!(
        "DROP SCHEMA {src} CASCADE; DROP SCHEMA {tgt} CASCADE; \
         DELETE FROM querylab.schema_migrations WHERE project = '{project_name}';"
    ))
    .execute(&mut db)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn diff_validation_and_missing_schemas(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let reader = create_connection(&app, &token, sample_connection("Reader")).await;
    let body = |src: &str, tgt: &str| json!({ "source": { "connection_id": reader, "schema": src }, "target": { "connection_id": reader, "schema": tgt } });
    assert_eq!(
        call(
            &app,
            &token,
            "POST",
            "/api/schema-diff",
            Some(body("library", "library"))
        )
        .await
        .0,
        422
    );
    let (status, res) = call(
        &app,
        &token,
        "POST",
        "/api/schema-diff",
        Some(body("nope", "library")),
    )
    .await;
    assert_eq!(status, 404, "{res}");
    // A missing target schema is created by the script.
    let (status, diff) = call(
        &app,
        &token,
        "POST",
        "/api/schema-diff",
        Some(body("library", "library_copy")),
    )
    .await;
    assert_eq!(status, 200, "{diff}");
    let changes = diff["changes"].as_array().unwrap();
    assert_eq!(changes[0]["sql"], "CREATE SCHEMA library_copy;");
    assert!(changes.iter().all(|c| c["destructive"] == false));
    assert_eq!(
        changes
            .iter()
            .filter(|c| c["object_type"] == "table" && c["action"] == "create")
            .count(),
        6
    );
}

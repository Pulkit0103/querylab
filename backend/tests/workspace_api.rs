//! Phase 5: history filters and pins, retention, tags, nested folders and
//! the synced tab session.

mod common;

use axum::Router;
use common::{api, register, send, send_full, session_token};
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

async fn user_id(app: &Router, token: &str) -> Uuid {
    let (_, me) = call(app, token, "GET", "/api/auth/me", None).await;
    me["id"].as_str().unwrap().parse().unwrap()
}

/// Inserts a history row `age_days` old with the given duration and kind.
async fn history(
    pool: &PgPool,
    user: Uuid,
    sql: &str,
    ms: i64,
    age_days: i32,
    kind: &str,
    status: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO query_history (id, user_id, sql_content, execution_time_ms, status, kind, executed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now() - make_interval(days => $7))",
    )
    .bind(id)
    .bind(user)
    .bind(sql)
    .bind(ms)
    .bind(status)
    .bind(kind)
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn sqls(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["sql_content"].as_str().unwrap().to_string())
        .collect()
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn history_filters_pins_and_retention(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool.clone());
    let token = register(&app, "a@example.com").await;
    let me = user_id(&app, &token).await;

    // Newest first: q0 (today) ... q9 (9 days ago), durations 0,100,...,900.
    let mut ids = Vec::new();
    for i in 0..10 {
        let kind = if i % 3 == 0 { "data_apply" } else { "query" };
        let status = if i == 4 { "error" } else { "success" };
        ids.push(
            history(
                &pool,
                me,
                &format!("select {i}"),
                i64::from(i) * 100,
                i,
                kind,
                status,
            )
            .await,
        );
    }

    // Duration range, paginated two at a time.
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut path = "/api/history?limit=2&min_ms=200&max_ms=700".to_string();
        if let Some(c) = &cursor {
            path.push_str(&format!("&before={c}"));
        }
        let (status, page) = call(&app, &token, "GET", &path, None).await;
        assert_eq!(status, 200, "{page}");
        seen.extend(sqls(&page));
        match page["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    assert_eq!(
        seen,
        ["select 2", "select 3", "select 4", "select 5", "select 6", "select 7"]
    );

    // Date range combined with kind and status.
    let from = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let to = (chrono::Utc::now() - chrono::Duration::hours(36)).to_rfc3339();
    let enc = |s: &str| s.replace('+', "%2B").replace(':', "%3A");
    let (_, page) = call(
        &app,
        &token,
        "GET",
        &format!("/api/history?from={}&to={}", enc(&from), enc(&to)),
        None,
    )
    .await;
    assert_eq!(
        sqls(&page),
        ["select 2", "select 3", "select 4", "select 5", "select 6"]
    );
    let (_, page) = call(
        &app,
        &token,
        "GET",
        &format!(
            "/api/history?from={}&to={}&kind=data_apply",
            enc(&from),
            enc(&to)
        ),
        None,
    )
    .await;
    assert_eq!(sqls(&page), ["select 3", "select 6"]);
    assert_eq!(page["items"][0]["kind"], "data_apply");
    let (_, page) = call(
        &app,
        &token,
        "GET",
        "/api/history?status=error&max_ms=500",
        None,
    )
    .await;
    assert_eq!(sqls(&page), ["select 4"]);

    // Invalid ranges are rejected.
    assert_eq!(
        call(&app, &token, "GET", "/api/history?min_ms=9&max_ms=1", None)
            .await
            .0,
        422
    );

    // Pins.
    let (status, pinned) = call(
        &app,
        &token,
        "PUT",
        &format!("/api/history/{}/pin", ids[8]),
        Some(json!({ "pinned": true })),
    )
    .await;
    assert_eq!(status, 200, "{pinned}");
    assert_eq!(pinned["pinned"], true);
    call(
        &app,
        &token,
        "PUT",
        &format!("/api/history/{}/pin", ids[1]),
        Some(json!({ "pinned": true })),
    )
    .await;
    let (_, page) = call(&app, &token, "GET", "/api/history?pinned=true", None).await;
    assert_eq!(sqls(&page), ["select 1", "select 8"]);
    call(
        &app,
        &token,
        "PUT",
        &format!("/api/history/{}/pin", ids[1]),
        Some(json!({ "pinned": false })),
    )
    .await;
    let (_, page) = call(&app, &token, "GET", "/api/history?pinned=true", None).await;
    assert_eq!(sqls(&page), ["select 8"]);

    // Other users cannot pin.
    let other = register(&app, "b@example.com").await;
    assert_eq!(
        call(
            &app,
            &other,
            "PUT",
            &format!("/api/history/{}/pin", ids[0]),
            Some(json!({ "pinned": true }))
        )
        .await
        .0,
        404
    );

    // Retention: 5 days. Older unpinned entries go; pinned "select 8" stays;
    // the other user's old history follows their own (default 90-day) setting.
    let other_id = user_id(&app, &other).await;
    history(&pool, other_id, "theirs", 1, 30, "query", "success").await;
    let (status, settings) = call(&app, &token, "GET", "/api/workspace/settings", None).await;
    assert_eq!(status, 200);
    assert_eq!(settings["history_retention_days"], 90);
    let (status, _) = call(
        &app,
        &token,
        "PUT",
        "/api/workspace/settings",
        Some(json!({ "history_retention_days": 5 })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        call(
            &app,
            &token,
            "PUT",
            "/api/workspace/settings",
            Some(json!({ "history_retention_days": 0 }))
        )
        .await
        .0,
        422
    );
    let removed = querylab_api::services::library::prune_history(&pool)
        .await
        .unwrap();
    assert_eq!(removed, 4, "select 5, 6, 7 and 9");
    let (_, page) = call(&app, &token, "GET", "/api/history", None).await;
    assert_eq!(
        sqls(&page),
        ["select 0", "select 1", "select 2", "select 3", "select 4", "select 8"]
    );
    let (_, theirs) = call(&app, &other, "GET", "/api/history", None).await;
    assert_eq!(sqls(&theirs), ["theirs"]);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn saved_query_tags_and_pins(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let save =
        |name: &str, tags: Value| json!({ "name": name, "sql_content": "select 1", "tags": tags });

    let (status, a) = call(
        &app,
        &token,
        "POST",
        "/api/queries",
        Some(save("A", json!([" Reports", "reports", "KPI"]))),
    )
    .await;
    assert_eq!(status, 201, "{a}");
    assert_eq!(a["tags"], json!(["reports", "kpi"]));
    assert_eq!(a["pinned"], false);
    call(
        &app,
        &token,
        "POST",
        "/api/queries",
        Some(save("B", json!(["reports"]))),
    )
    .await;
    let (_, c) = call(
        &app,
        &token,
        "POST",
        "/api/queries",
        Some(save("C", json!([]))),
    )
    .await;
    let too_many: Vec<String> = (0..21).map(|i| format!("t{i}")).collect();
    assert_eq!(
        call(
            &app,
            &token,
            "POST",
            "/api/queries",
            Some(save("D", json!(too_many)))
        )
        .await
        .0,
        422
    );

    let names = |list: &Value| -> Vec<String> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|q| q["name"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, list) = call(&app, &token, "GET", "/api/queries?tags=reports", None).await;
    assert_eq!(names(&list).len(), 2);
    let (_, list) = call(&app, &token, "GET", "/api/queries?tags=Reports,kpi", None).await;
    assert_eq!(names(&list), ["A"], "every tag must match");

    let (_, tags) = call(&app, &token, "GET", "/api/tags", None).await;
    assert_eq!(
        tags,
        json!([{ "tag": "reports", "count": 2 }, { "tag": "kpi", "count": 1 }])
    );

    // Tags are replaced on update; absent tags are kept.
    let (_, updated) = call(
        &app,
        &token,
        "PUT",
        &format!("/api/queries/{}", a["id"].as_str().unwrap()),
        Some(json!({ "tags": ["ops"] })),
    )
    .await;
    assert_eq!(updated["tags"], json!(["ops"]));
    let (_, renamed) = call(
        &app,
        &token,
        "PUT",
        &format!("/api/queries/{}", a["id"].as_str().unwrap()),
        Some(json!({ "name": "A2" })),
    )
    .await;
    assert_eq!(renamed["tags"], json!(["ops"]));

    // Pinning lists C first and does not count as an edit.
    let c_id = c["id"].as_str().unwrap();
    let (status, pinned) = call(
        &app,
        &token,
        "PUT",
        &format!("/api/queries/{c_id}/pin"),
        Some(json!({ "pinned": true })),
    )
    .await;
    assert_eq!(status, 200, "{pinned}");
    assert_eq!(pinned["pinned"], true);
    assert_eq!(pinned["updated_at"], c["updated_at"]);
    let (_, list) = call(&app, &token, "GET", "/api/queries", None).await;
    assert_eq!(names(&list)[0], "C");
    let (_, list) = call(&app, &token, "GET", "/api/queries?pinned=true", None).await;
    assert_eq!(names(&list), ["C"]);

    let other = register(&app, "b@example.com").await;
    assert_eq!(
        call(
            &app,
            &other,
            "PUT",
            &format!("/api/queries/{c_id}/pin"),
            Some(json!({ "pinned": true }))
        )
        .await
        .0,
        404
    );
    let (_, theirs) = call(&app, &other, "GET", "/api/tags", None).await;
    assert_eq!(theirs, json!([]));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn folders_nest_without_cycles(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let create = |name: &str, parent: Option<&str>| json!({ "name": name, "parent_id": parent });
    let id_of = |v: &Value| v["id"].as_str().unwrap().to_string();

    let (status, a) = call(
        &app,
        &token,
        "POST",
        "/api/collections",
        Some(create("A", None)),
    )
    .await;
    assert_eq!(status, 201, "{a}");
    let (_, b) = call(
        &app,
        &token,
        "POST",
        "/api/collections",
        Some(create("B", Some(&id_of(&a)))),
    )
    .await;
    assert_eq!(b["parent_id"], a["id"]);
    let (status, c) = call(
        &app,
        &token,
        "POST",
        "/api/collections",
        Some(create("C", Some(&id_of(&b)))),
    )
    .await;
    assert_eq!(status, 201, "depth 3 is allowed: {c}");
    let (status, too_deep) = call(
        &app,
        &token,
        "POST",
        "/api/collections",
        Some(create("D", Some(&id_of(&c)))),
    )
    .await;
    assert_eq!(status, 422, "{too_deep}");

    // Same name is fine under a different parent, not among siblings.
    assert_eq!(
        call(
            &app,
            &token,
            "POST",
            "/api/collections",
            Some(create("B", None))
        )
        .await
        .0,
        201
    );
    assert_eq!(
        call(
            &app,
            &token,
            "POST",
            "/api/collections",
            Some(create("B", Some(&id_of(&a))))
        )
        .await
        .0,
        409
    );

    // Cycles: into itself or a descendant.
    let path = |v: &Value| format!("/api/collections/{}", id_of(v));
    for target in [&a, &b, &c] {
        let (status, body) = call(
            &app,
            &token,
            "PUT",
            &path(&a),
            Some(json!({ "parent_id": id_of(target) })),
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }

    // Moving a two-level subtree (B > C) under a top-level folder X keeps
    // depth 3; under X > Y it would be 4.
    let (_, x) = call(
        &app,
        &token,
        "POST",
        "/api/collections",
        Some(create("X", None)),
    )
    .await;
    let (_, y) = call(
        &app,
        &token,
        "POST",
        "/api/collections",
        Some(create("Y", Some(&id_of(&x)))),
    )
    .await;
    let (status, moved) = call(
        &app,
        &token,
        "PUT",
        &path(&b),
        Some(json!({ "parent_id": id_of(&x) })),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["parent_id"], x["id"]);
    assert_eq!(
        call(
            &app,
            &token,
            "PUT",
            &path(&b),
            Some(json!({ "parent_id": id_of(&y) }))
        )
        .await
        .0,
        422
    );

    // Rename only, and move to the top level.
    let (_, renamed) = call(
        &app,
        &token,
        "PUT",
        &path(&b),
        Some(json!({ "name": "B2" })),
    )
    .await;
    assert_eq!(
        (renamed["name"].as_str(), renamed["parent_id"].as_str()),
        (Some("B2"), x["id"].as_str())
    );
    let (_, top) = call(
        &app,
        &token,
        "PUT",
        &path(&b),
        Some(json!({ "parent_id": null })),
    )
    .await;
    assert!(top["parent_id"].is_null());

    // Deleting a folder removes its subfolders; queries become unfiled.
    let (_, q) = call(
        &app,
        &token,
        "POST",
        "/api/queries",
        Some(json!({ "name": "in C", "sql_content": "select 1", "collection_id": id_of(&c) })),
    )
    .await;
    assert_eq!(call(&app, &token, "DELETE", &path(&b), None).await.0, 204);
    let (_, folders) = call(&app, &token, "GET", "/api/collections", None).await;
    assert!(!folders
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["id"] == c["id"]));
    let (_, kept) = call(
        &app,
        &token,
        "GET",
        &format!("/api/queries/{}", id_of(&q)),
        None,
    )
    .await;
    assert!(kept["collection_id"].is_null());

    // Another user's folder is not a valid parent.
    let other = register(&app, "b@example.com").await;
    assert_eq!(
        call(
            &app,
            &other,
            "POST",
            "/api/collections",
            Some(create("Z", Some(&id_of(&a))))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &app,
            &other,
            "PUT",
            &path(&a),
            Some(json!({ "name": "stolen" }))
        )
        .await
        .0,
        404
    );
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn tab_sessions_follow_the_user(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;

    let (status, empty) = call(&app, &token, "GET", "/api/workspace/tabs", None).await;
    assert_eq!(status, 200);
    assert!(empty["session"].is_null() && empty["updated_at"].is_null());

    let session = json!({
        "tabs": [
            { "id": "t1", "title": "Revenue", "sql": "select 1", "saved_query_id": null, "saved_sql": null },
            { "id": "t2", "title": "Draft", "sql": "select 2" }
        ],
        "active_tab_id": "t2",
        "active_connection_id": null
    });
    let (status, saved) = call(
        &app,
        &token,
        "PUT",
        "/api/workspace/tabs",
        Some(session.clone()),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    assert!(saved["updated_at"].is_string());

    // A second login (another browser) gets the same tabs.
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
    let second = session_token(&headers).unwrap();
    let (_, restored) = call(&app, &second, "GET", "/api/workspace/tabs", None).await;
    assert_eq!(restored["session"]["tabs"][0]["title"], "Revenue");
    assert_eq!(restored["session"]["active_tab_id"], "t2");
    assert_eq!(restored["session"]["tabs"][1]["saved_sql"], Value::Null);

    // Other users see nothing; invalid sessions are rejected.
    let other = register(&app, "b@example.com").await;
    let (_, theirs) = call(&app, &other, "GET", "/api/workspace/tabs", None).await;
    assert!(theirs["session"].is_null());
    let mut bad = session.clone();
    bad["active_tab_id"] = json!("missing");
    assert_eq!(
        call(&app, &token, "PUT", "/api/workspace/tabs", Some(bad))
            .await
            .0,
        422
    );
    assert_eq!(
        call(
            &app,
            &token,
            "PUT",
            "/api/workspace/tabs",
            Some(json!({ "tabs": [] }))
        )
        .await
        .0,
        422
    );

    // Size limits: a body over 2 MiB is refused before parsing.
    let huge = "x".repeat(2 * 1024 * 1024);
    let big = json!({ "tabs": [{ "id": "t", "title": "big", "sql": huge }] });
    assert_eq!(
        call(&app, &token, "PUT", "/api/workspace/tabs", Some(big))
            .await
            .0,
        413
    );
    let many: Vec<Value> = (0..51)
        .map(|i| json!({ "id": format!("t{i}"), "title": "", "sql": "" }))
        .collect();
    assert_eq!(
        call(
            &app,
            &token,
            "PUT",
            "/api/workspace/tabs",
            Some(json!({ "tabs": many }))
        )
        .await
        .0,
        422
    );

    // The stored session is unchanged by rejected writes.
    let (_, still) = call(&app, &token, "GET", "/api/workspace/tabs", None).await;
    assert_eq!(still["session"], session_with_defaults(&session));
}

/// The session as stored: optional fields filled with nulls.
fn session_with_defaults(session: &Value) -> Value {
    let mut s = session.clone();
    for tab in s["tabs"].as_array_mut().unwrap() {
        for field in ["saved_query_id", "saved_sql"] {
            if tab.get(field).is_none() {
                tab[field] = Value::Null;
            }
        }
    }
    s
}

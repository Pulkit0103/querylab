//! Query history, saved queries, collections and stats.

mod common;

use common::{api, register, sample_connection, send};
use serde_json::json;
use sqlx::PgPool;

async fn setup(pool: PgPool) -> (axum::Router, String, String) {
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
    (app, token, created["id"].as_str().unwrap().to_string())
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn executions_are_recorded_in_history(pool: PgPool) {
    let (app, token, id) = setup(pool).await;
    let run = |sql: &str| {
        api(
            "POST",
            "/api/query/execute",
            Some(json!({ "connection_id": id, "query": sql })),
            Some(&token),
        )
    };

    let (status, ok) = send(&app, run("SELECT * FROM departments")).await;
    assert_eq!(status, 200);
    assert!(ok["history_id"].is_string());
    send(&app, run("SELECT nope FROM departments")).await;
    send(&app, run("SELECT 100% match")).await;

    let (status, page) = send(&app, api("GET", "/api/history", None, Some(&token))).await;
    assert_eq!(status, 200);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[2]["status"], "success", "newest first");
    assert_eq!(items[2]["row_count"], 5);
    assert_eq!(items[2]["connection_name"], "Sample");
    assert_eq!(items[1]["status"], "error");
    assert!(items[1]["error_message"].as_str().unwrap().contains("nope"));

    // Filters: status, literal search (wildcards escaped), pagination.
    let (_, errors) = send(
        &app,
        api("GET", "/api/history?status=error", None, Some(&token)),
    )
    .await;
    assert_eq!(errors["items"].as_array().unwrap().len(), 2);
    let (_, found) = send(
        &app,
        api("GET", "/api/history?q=100%25", None, Some(&token)),
    )
    .await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let (_, first) = send(&app, api("GET", "/api/history?limit=2", None, Some(&token))).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = send(
        &app,
        api(
            "GET",
            &format!("/api/history?limit=2&before={cursor}"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert!(second["next_cursor"].is_null());

    // Get, delete and isolation.
    let entry_id = items[0]["id"].as_str().unwrap();
    assert_eq!(
        send(
            &app,
            api(
                "GET",
                &format!("/api/history/{entry_id}"),
                None,
                Some(&token)
            )
        )
        .await
        .0,
        200
    );
    let other = register(&app, "b@example.com").await;
    assert_eq!(
        send(
            &app,
            api(
                "GET",
                &format!("/api/history/{entry_id}"),
                None,
                Some(&other)
            )
        )
        .await
        .0,
        404
    );
    let (_, empty) = send(&app, api("GET", "/api/history", None, Some(&other))).await;
    assert!(empty["items"].as_array().unwrap().is_empty());
    assert_eq!(
        send(
            &app,
            api(
                "DELETE",
                &format!("/api/history/{entry_id}"),
                None,
                Some(&token)
            )
        )
        .await
        .0,
        204
    );
    let (_, cleared) = send(&app, api("DELETE", "/api/history", None, Some(&token))).await;
    assert_eq!(cleared["deleted"], 2);

    // Stats reflect activity.
    send(&app, run("SELECT 1")).await;
    let (status, stats) = send(&app, api("GET", "/api/stats?days=7", None, Some(&token))).await;
    assert_eq!(status, 200, "{stats}");
    assert_eq!(stats["totals"]["executions"], 1);
    assert_eq!(stats["totals"]["connections"], 1);
    assert_eq!(stats["daily"].as_array().unwrap().len(), 7);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn saved_queries_and_collections(pool: PgPool) {
    let (app, token, conn_id) = setup(pool).await;

    let (status, coll) = send(
        &app,
        api(
            "POST",
            "/api/collections",
            Some(json!({ "name": "Reports" })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 201);
    let coll_id = coll["id"].as_str().unwrap().to_string();
    assert_eq!(
        send(
            &app,
            api(
                "POST",
                "/api/collections",
                Some(json!({ "name": "Reports" })),
                Some(&token)
            )
        )
        .await
        .0,
        409
    );

    let (status, saved) = send(
        &app,
        api(
            "POST",
            "/api/queries",
            Some(json!({
                "name": "Revenue by country", "description": "Monthly report",
                "sql_content": "SELECT * FROM sales.revenue_by_country",
                "connection_id": conn_id, "collection_id": coll_id
            })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 201, "{saved}");
    let qid = saved["id"].as_str().unwrap().to_string();
    let path = format!("/api/queries/{qid}");

    // Search and collection filter.
    let (_, found) = send(
        &app,
        api("GET", "/api/queries?q=revenue", None, Some(&token)),
    )
    .await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    let (_, by_coll) = send(
        &app,
        api(
            "GET",
            &format!("/api/queries?collection_id={coll_id}"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(by_coll.as_array().unwrap().len(), 1);
    let (_, colls) = send(&app, api("GET", "/api/collections", None, Some(&token))).await;
    assert_eq!(colls[0]["query_count"], 1);

    // Partial update: absent fields stay, explicit null clears.
    let (status, updated) = send(
        &app,
        api(
            "PUT",
            &path,
            Some(json!({ "sql_content": "SELECT 1", "collection_id": null })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(updated["name"], "Revenue by country");
    assert_eq!(updated["sql_content"], "SELECT 1");
    assert!(updated["collection_id"].is_null());
    assert_eq!(updated["connection_id"], conn_id.as_str());

    // Other users cannot see, change or reference these resources.
    let other = register(&app, "b@example.com").await;
    assert_eq!(
        send(&app, api("GET", &path, None, Some(&other))).await.0,
        404
    );
    assert_eq!(
        send(
            &app,
            api("PUT", &path, Some(json!({ "name": "x" })), Some(&other))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        send(&app, api("DELETE", &path, None, Some(&other))).await.0,
        404
    );
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/queries",
            Some(json!({ "name": "steal", "sql_content": "select 1", "collection_id": coll_id })),
            Some(&other),
        ),
    )
    .await;
    assert_eq!(status, 404, "cannot file into another user's collection");
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/queries",
            Some(json!({ "name": "steal", "sql_content": "select 1", "connection_id": conn_id })),
            Some(&other),
        ),
    )
    .await;
    assert_eq!(status, 404, "cannot attach another user's connection");

    // Deleting a collection keeps its queries; deleting a connection unlinks them.
    send(
        &app,
        api(
            "PUT",
            &path,
            Some(json!({ "collection_id": coll_id })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(
        send(
            &app,
            api(
                "DELETE",
                &format!("/api/collections/{coll_id}"),
                None,
                Some(&token)
            )
        )
        .await
        .0,
        204
    );
    assert_eq!(
        send(
            &app,
            api(
                "DELETE",
                &format!("/api/connections/{conn_id}"),
                None,
                Some(&token)
            )
        )
        .await
        .0,
        204
    );
    let (_, after) = send(&app, api("GET", &path, None, Some(&token))).await;
    assert!(after["collection_id"].is_null());
    assert!(after["connection_id"].is_null());

    assert_eq!(
        send(&app, api("DELETE", &path, None, Some(&token))).await.0,
        204
    );
    assert_eq!(
        send(&app, api("GET", &path, None, Some(&token))).await.0,
        404
    );
}

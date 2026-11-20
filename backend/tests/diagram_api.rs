//! Phase 6: schema graph with inferred cardinality and saved diagram layouts
//! (against `scripts/sample-db`).

mod common;

use axum::Router;
use common::{api, register, sample_connection, send};
use serde_json::{json, Value};
use sqlx::PgPool;

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

async fn setup(pool: PgPool) -> (Router, String, String) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let (status, created) = call(
        &app,
        &token,
        "POST",
        "/api/connections",
        Some(sample_connection("Sample")),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    (app, token, id)
}

fn edge<'a>(graph: &'a Value, constraint: &str) -> &'a Value {
    graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["constraint"] == constraint)
        .unwrap_or_else(|| panic!("{constraint} missing: {graph}"))
}

fn table_names(graph: &Value) -> Vec<&str> {
    graph["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect()
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn graph_infers_cardinality(pool: PgPool) {
    let (app, token, id) = setup(pool).await;
    let base = format!("/api/connections/{id}/schema-graph");

    let (status, graph) = call(&app, &token, "GET", &format!("{base}?schema=library"), None).await;
    assert_eq!(status, 200, "{graph}");
    assert_eq!(
        table_names(&graph),
        [
            "author_profiles",
            "authors",
            "books",
            "copies",
            "editions",
            "loans"
        ]
    );
    assert_eq!(graph["total_tables"], 6);
    assert_eq!(graph["truncated"], false);
    assert_eq!(graph["hidden_edges"], 0);

    // Shared primary key: one-to-one, required.
    let profile = edge(&graph, "author_profiles_author_id_fkey");
    assert_eq!(profile["cardinality"], "one_to_one");
    assert_eq!(profile["optional"], false);
    assert_eq!(profile["on_delete"], "cascade");
    assert_eq!(profile["from_table"], "author_profiles");
    assert_eq!(profile["to_table"], "authors");

    let author = edge(&graph, "books_author_id_fkey");
    assert_eq!(author["cardinality"], "one_to_many");
    assert_eq!(author["optional"], false);
    let translator = edge(&graph, "books_translator_id_fkey");
    assert_eq!(translator["cardinality"], "one_to_many");
    assert_eq!(translator["optional"], true);
    assert_eq!(translator["on_delete"], "set_null");

    // Unique nullable self-reference.
    let sequel = edge(&graph, "books_sequel_of_fkey");
    assert_eq!(sequel["cardinality"], "one_to_one");
    assert_eq!(sequel["optional"], true);
    assert_eq!(sequel["from_table"], "books");
    assert_eq!(sequel["to_table"], "books");

    // Part of a composite primary key: still many editions per book.
    assert_eq!(
        edge(&graph, "editions_book_id_fkey")["cardinality"],
        "one_to_many"
    );
    let copies = edge(&graph, "copies_edition_fkey");
    assert_eq!(copies["from_columns"], json!(["book_id", "edition_no"]));
    assert_eq!(copies["to_columns"], json!(["book_id", "edition_no"]));
    assert_eq!(copies["cardinality"], "one_to_many");
    // A partial unique index does not make it one-to-one.
    assert_eq!(
        edge(&graph, "loans_copy_id_fkey")["cardinality"],
        "one_to_many"
    );

    // Column key markers.
    let books = graph["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "books")
        .unwrap();
    let col = |name: &str| {
        books["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .cloned()
            .unwrap()
    };
    assert_eq!(col("id")["primary_key"], true);
    assert_eq!(col("author_id")["foreign_key"], true);
    assert_eq!(col("author_id")["nullable"], false);
    assert_eq!(col("sequel_of")["unique"], true);
    assert_eq!(col("title")["data_type"], "text");
    assert_eq!(col("title")["foreign_key"], false);

    // Cross-schema references are counted, not drawn: sales.orders -> public.employees.
    let (_, sales) = call(&app, &token, "GET", &format!("{base}?schema=sales"), None).await;
    assert_eq!(table_names(&sales), ["customers", "orders"]);
    assert_eq!(sales["hidden_edges"], 1);
    assert_eq!(
        edge(&sales, "orders_customer_id_fkey")["cardinality"],
        "one_to_many"
    );
    // Views on request.
    let (_, with_views) = call(
        &app,
        &token,
        "GET",
        &format!("{base}?schema=sales&include_views=true"),
        None,
    )
    .await;
    assert!(table_names(&with_views).contains(&"revenue_by_country"));

    // A selection keeps only edges inside it.
    let (_, picked) = call(
        &app,
        &token,
        "GET",
        &format!("{base}?schema=library&tables=books,authors,missing"),
        None,
    )
    .await;
    assert_eq!(table_names(&picked), ["authors", "books"]);
    assert_eq!(picked["edges"].as_array().unwrap().len(), 3, "{picked}");
    // profiles -> authors and editions -> books are outside the selection.
    assert_eq!(picked["hidden_edges"], 2);

    // Large schemas start with the most connected tables.
    let (_, top) = call(
        &app,
        &token,
        "GET",
        &format!("{base}?schema=library&limit=2"),
        None,
    )
    .await;
    assert_eq!(top["truncated"], true);
    assert_eq!(top["total_tables"], 6);
    assert_eq!(table_names(&top), ["authors", "books"]);

    // Errors.
    let (status, _) = call(&app, &token, "GET", &format!("{base}?schema=nope"), None).await;
    assert_eq!(status, 404);
    let (status, _) = call(
        &app,
        &token,
        "GET",
        &format!("{base}?schema=library&limit=0"),
        None,
    )
    .await;
    assert_eq!(status, 422);
    let (status, _) = call(
        &app,
        &token,
        "GET",
        &format!("{base}?schema=library&tables=,"),
        None,
    )
    .await;
    assert_eq!(status, 422);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn layouts_are_saved_per_schema_and_owner(pool: PgPool) {
    let (app, token, id) = setup(pool.clone()).await;
    let path = |schema: &str| format!("/api/connections/{id}/diagram-layouts/{schema}");

    let (status, empty) = call(&app, &token, "GET", &path("library"), None).await;
    assert_eq!(status, 200, "{empty}");
    assert_eq!(empty, json!({ "positions": {}, "updated_at": null }));

    let positions =
        json!({ "books": { "x": 10.5, "y": -20.0 }, "authors": { "x": 300.0, "y": 0.0 } });
    let (status, saved) = call(
        &app,
        &token,
        "PUT",
        &path("library"),
        Some(json!({ "positions": positions })),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    assert!(saved["updated_at"].is_string());
    let (_, loaded) = call(&app, &token, "GET", &path("library"), None).await;
    assert_eq!(loaded["positions"], positions);
    // Another schema has its own layout.
    let (_, other) = call(&app, &token, "GET", &path("sales"), None).await;
    assert_eq!(other["positions"], json!({}));

    // Overwrite.
    let (_, _) = call(
        &app,
        &token,
        "PUT",
        &path("library"),
        Some(json!({ "positions": { "books": { "x": 1, "y": 2 } } })),
    )
    .await;
    let (_, loaded) = call(&app, &token, "GET", &path("library"), None).await;
    assert_eq!(
        loaded["positions"],
        json!({ "books": { "x": 1.0, "y": 2.0 } })
    );

    // Validation.
    let (status, _) = call(
        &app,
        &token,
        "PUT",
        &path("library"),
        Some(json!({ "positions": { "books": { "x": 1e12, "y": 0 } } })),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _) = call(
        &app,
        &token,
        "PUT",
        &path("library"),
        Some(json!({ "positions": { "books": { "x": "a", "y": 0 } } })),
    )
    .await;
    assert_eq!(status, 400);

    // Other users see nothing and cannot write.
    let mallory = register(&app, "mallory@example.com").await;
    for method in ["GET", "PUT", "DELETE"] {
        let body = (method == "PUT").then(|| json!({ "positions": {} }));
        let (status, res) = call(&app, &mallory, method, &path("library"), body).await;
        assert_eq!(status, 404, "{method}: {res}");
    }
    let (_, graph) = call(
        &app,
        &mallory,
        "GET",
        &format!("/api/connections/{id}/schema-graph?schema=library"),
        None,
    )
    .await;
    assert_eq!(graph["error"]["code"], "not_found");

    // Reset.
    let (status, _) = call(&app, &token, "DELETE", &path("library"), None).await;
    assert_eq!(status, 204);
    let (_, loaded) = call(&app, &token, "GET", &path("library"), None).await;
    assert_eq!(loaded["updated_at"], Value::Null);

    // Deleting the connection removes its layouts.
    call(
        &app,
        &token,
        "PUT",
        &path("library"),
        Some(json!({ "positions": { "books": { "x": 0, "y": 0 } } })),
    )
    .await;
    let (status, _) = call(
        &app,
        &token,
        "DELETE",
        &format!("/api/connections/{id}"),
        None,
    )
    .await;
    assert_eq!(status, 204);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM diagram_layouts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

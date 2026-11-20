//! Advanced explorer: constraints, functions, triggers, sequences, types,
//! catalog search and DDL generation (against `scripts/sample-db`).

mod common;

use common::{api, register, sample_connection, send};
use serde_json::{json, Value};
use sqlx::{Connection, PgConnection, PgPool};

async fn setup(pool: PgPool, owner: bool) -> (axum::Router, String, String) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let mut body = sample_connection("Sample");
    if owner {
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

async fn get(app: &axum::Router, token: &str, path: &str) -> (u16, Value) {
    let (status, body) = send(app, api("GET", path, None, Some(token))).await;
    (status.as_u16(), body)
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn constraints_functions_and_triggers(pool: PgPool) {
    let (app, token, id) = setup(pool, false).await;
    let base = format!("/api/connections/{id}");

    // Constraints, ordered PK, unique, check; NOT NULL is not listed.
    let (status, cons) = get(
        &app,
        &token,
        &format!("{base}/tables/products/constraints?schema=inventory"),
    )
    .await;
    assert_eq!(status, 200, "{cons}");
    let kinds: Vec<&str> = cons
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["primary_key", "unique", "check"]);
    assert_eq!(cons[1]["name"], "products_sku_key");
    assert_eq!(cons[1]["columns"], json!(["sku"]));
    assert_eq!(cons[2]["definition"], "CHECK (price >= 0::numeric)");
    let (_, emp) = get(
        &app,
        &token,
        &format!("{base}/tables/employees/constraints"),
    )
    .await;
    let fk = emp
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "foreign_key")
        .unwrap();
    assert_eq!(fk["referenced_table"], "public.departments");

    // Functions (extension-owned functions such as pgcrypto's are excluded).
    let (status, funcs) = get(&app, &token, &format!("{base}/functions?schema=inventory")).await;
    assert_eq!(status, 200, "{funcs}");
    let find = |name: &str| {
        funcs
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap()
            .clone()
    };
    let value = find("stock_value");
    assert_eq!(value["kind"], "function");
    assert_eq!(value["arguments"], "p_status inventory.stock_status");
    assert_eq!(value["result"], "numeric");
    assert_eq!(value["volatility"], "stable");
    assert_eq!(value["language"], "sql");
    assert_eq!(value["comment"], "Value of stock on hand");
    let restock = find("restock");
    assert_eq!(restock["kind"], "procedure");
    assert!(restock["result"].is_null());
    assert_eq!(find("touch_updated_at")["result"], "trigger");

    let (status, detail) = get(&app, &token, &format!("{base}/functions/{}", value["oid"])).await;
    assert_eq!(status, 200, "{detail}");
    assert!(detail["definition"]
        .as_str()
        .unwrap()
        .starts_with("CREATE OR REPLACE FUNCTION inventory.stock_value("));
    // System functions and unknown oids are not exposed.
    let (_, now_oid) = send(
        &app,
        api(
            "POST",
            "/api/query/execute",
            Some(json!({ "connection_id": id, "query": "SELECT 'now'::regproc::oid::bigint AS oid" })),
            Some(&token),
        ),
    )
    .await;
    let system_oid = now_oid["rows"][0][0].as_i64().unwrap();
    assert_eq!(
        get(&app, &token, &format!("{base}/functions/{system_oid}"))
            .await
            .0,
        404
    );
    assert_eq!(
        get(&app, &token, &format!("{base}/functions/1")).await.0,
        404
    );

    // Triggers.
    let (status, trg) = get(
        &app,
        &token,
        &format!("{base}/triggers?schema=inventory&table=products"),
    )
    .await;
    assert_eq!(status, 200, "{trg}");
    assert_eq!(trg.as_array().unwrap().len(), 1);
    assert_eq!(trg[0]["name"], "products_touch");
    assert_eq!(trg[0]["timing"], "BEFORE");
    assert_eq!(trg[0]["events"], json!(["UPDATE"]));
    assert_eq!(trg[0]["level"], "ROW");
    assert_eq!(trg[0]["function"], "inventory.touch_updated_at");
    assert_eq!(trg[0]["enabled"], "enabled");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn sequences_types_and_search(pool: PgPool) {
    let (app, token, id) = setup(pool, false).await;
    let base = format!("/api/connections/{id}");

    let (status, objects) = get(&app, &token, &format!("{base}/objects?schema=inventory")).await;
    assert_eq!(status, 200, "{objects}");
    let seqs = objects["sequences"].as_array().unwrap();
    let sku = seqs.iter().find(|s| s["name"] == "sku_seq").unwrap();
    assert_eq!(sku["start_value"], 1000);
    assert!(sku["owned_by"].is_null());
    let identity = seqs
        .iter()
        .find(|s| s["name"] == "products_id_seq")
        .unwrap();
    assert_eq!(identity["owned_by"], "inventory.products.id");
    let types = objects["types"].as_array().unwrap();
    let status_type = types.iter().find(|t| t["name"] == "stock_status").unwrap();
    assert_eq!(status_type["kind"], "enum");
    assert_eq!(
        status_type["values"],
        json!(["in_stock", "low", "out_of_stock"])
    );
    let domain = types.iter().find(|t| t["name"] == "positive_qty").unwrap();
    assert_eq!(domain["kind"], "domain");
    assert_eq!(domain["detail"], "integer CHECK (VALUE >= 0)");

    // Search: exact names first, then relations, columns, functions.
    let (status, hits) = get(&app, &token, &format!("{base}/search?q=price")).await;
    assert_eq!(status, 200, "{hits}");
    let hits = hits.as_array().unwrap();
    assert_eq!(hits[0]["kind"], "column");
    assert_eq!(hits[0]["name"], "price");
    assert_eq!(hits[0]["table"], "products");
    assert_eq!(hits[0]["detail"], "numeric(10,2)");
    assert!(hits.iter().any(|h| h["name"] == "price_with_tax"));
    let (_, hits) = get(&app, &token, &format!("{base}/search?q=ORDERS")).await;
    assert_eq!(hits[0]["kind"], "table");
    assert_eq!(hits[0]["schema"], "sales");
    let (_, hits) = get(&app, &token, &format!("{base}/search?q=stock_val")).await;
    assert!(hits
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["kind"] == "function"));
    // Wildcards are literal.
    let (_, hits) = get(&app, &token, &format!("{base}/search?q=%25")).await;
    assert!(hits.as_array().unwrap().is_empty(), "{hits}");
    assert_eq!(get(&app, &token, &format!("{base}/search?q=")).await.0, 422);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn generated_ddl_round_trips(pool: PgPool) {
    let (app, token, id) = setup(pool, true).await;
    let base = format!("/api/connections/{id}");
    let ddl_of = |schema: &str, table: &str| format!("{base}/tables/{table}/ddl?schema={schema}");

    let (status, original) = get(&app, &token, &ddl_of("inventory", "products")).await;
    assert_eq!(status, 200, "{original}");
    let original = original["ddl"].as_str().unwrap().to_string();
    for fragment in [
        "CREATE TABLE inventory.products (",
        "id bigint GENERATED ALWAYS AS IDENTITY NOT NULL",
        "sku text DEFAULT ('SKU-'::text || nextval('inventory.sku_seq'::regclass)) NOT NULL",
        "quantity inventory.positive_qty DEFAULT 0 NOT NULL",
        "GENERATED ALWAYS AS ((price * 1.2)) STORED",
        "CONSTRAINT products_pkey PRIMARY KEY (id)",
        "CONSTRAINT products_sku_key UNIQUE (sku)",
        "CREATE INDEX products_lower_name_idx ON inventory.products USING btree (lower(name));",
        "CREATE TRIGGER products_touch BEFORE UPDATE ON inventory.products",
        "COMMENT ON TABLE inventory.products IS 'Catalogue items';",
        "COMMENT ON COLUMN inventory.products.price IS 'Unit price excluding tax';",
    ] {
        assert!(
            original.contains(fragment),
            "missing {fragment:?} in\n{original}"
        );
    }

    // Apply the DDL in a scratch schema, regenerate it and compare.
    let scratch = format!("ddl_rt_{}", uuid::Uuid::new_v4().simple());
    let host = std::env::var("SAMPLE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("SAMPLE_PG_PORT").unwrap_or_else(|_| "5432".into());
    let mut owner = PgConnection::connect(&format!(
        "postgres://sample_owner:sample_owner@{host}:{port}/sampledb"
    ))
    .await
    .expect("connect as sample_owner");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {scratch}; {}",
        original.replace("inventory.products", &format!("{scratch}.products"))
    ))
    .execute(&mut owner)
    .await
    .expect("generated DDL applies cleanly");

    let (status, regenerated) = get(&app, &token, &ddl_of(&scratch, "products")).await;
    sqlx::raw_sql(&format!("DROP SCHEMA {scratch} CASCADE"))
        .execute(&mut owner)
        .await
        .unwrap();
    assert_eq!(status, 200, "{regenerated}");
    assert_eq!(
        regenerated["ddl"]
            .as_str()
            .unwrap()
            .replace(&format!("{scratch}.products"), "inventory.products"),
        original
    );

    // Views and materialized views.
    let (_, view) = get(&app, &token, &ddl_of("sales", "revenue_by_country")).await;
    assert_eq!(view["kind"], "view");
    assert!(view["ddl"]
        .as_str()
        .unwrap()
        .starts_with("CREATE VIEW sales.revenue_by_country AS\n"));
    let (_, matview) = get(&app, &token, &ddl_of("inventory", "stock_summary")).await;
    assert!(matview["ddl"]
        .as_str()
        .unwrap()
        .starts_with("CREATE MATERIALIZED VIEW inventory.stock_summary AS\n"));
    assert_eq!(get(&app, &token, &ddl_of("inventory", "nope")).await.0, 404);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn explorer_endpoints_are_owner_scoped(pool: PgPool) {
    let (app, _token, id) = setup(pool, false).await;
    let other = register(&app, "b@example.com").await;
    for path in [
        format!("/api/connections/{id}/tables/products/constraints?schema=inventory"),
        format!("/api/connections/{id}/tables/products/ddl?schema=inventory"),
        format!("/api/connections/{id}/functions"),
        format!("/api/connections/{id}/functions/1"),
        format!("/api/connections/{id}/triggers"),
        format!("/api/connections/{id}/objects"),
        format!("/api/connections/{id}/search?q=price"),
    ] {
        assert_eq!(get(&app, &other, &path).await.0, 404, "{path}");
    }
}

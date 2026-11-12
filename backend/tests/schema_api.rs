//! Schema explorer against the seeded sample database.

mod common;

use common::{api, register, sample_connection, send};
use sqlx::PgPool;

async fn setup(pool: PgPool) -> (axum::Router, String, String) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let (status, created) = send(
        &app,
        api(
            "POST",
            "/api/connections",
            Some(sample_connection("Sample")),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    (app, token, id)
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn lists_schemas_and_tables(pool: PgPool) {
    let (app, token, id) = setup(pool).await;

    let (status, schemas) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/schemas"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{schemas}");
    let names: Vec<_> = schemas
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names[0], "public", "public schema sorts first");
    assert!(names.contains(&"sales"));
    assert!(!names
        .iter()
        .any(|n| n.starts_with("pg_") || *n == "information_schema"));
    let sales = schemas
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "sales")
        .unwrap();
    assert_eq!(sales["table_count"], 3);

    let (status, tables) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/tables"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let find = |schema: &str, name: &str| {
        tables
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["schema"] == schema && t["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{schema}.{name} missing"))
    };
    let orders = find("sales", "orders");
    assert_eq!(orders["kind"], "table");
    assert_eq!(orders["estimated_rows"], 10000);
    assert!(orders["total_bytes"].as_i64().unwrap() > 0);
    assert_eq!(find("sales", "revenue_by_country")["kind"], "view");

    // Filter by schema.
    let (_, only_public) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/tables?schema=public"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert!(only_public
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["schema"] == "public"));
    assert_eq!(only_public.as_array().unwrap().len(), 2);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn columns_include_keys(pool: PgPool) {
    let (app, token, id) = setup(pool).await;
    let (status, cols) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/tables/employees/columns"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{cols}");
    let cols = cols.as_array().unwrap();
    assert_eq!(cols[0]["name"], "id");
    assert_eq!(cols[0]["is_primary_key"], true);
    assert_eq!(cols[0]["data_type"], "integer");
    assert!(cols[0]["default_value"]
        .as_str()
        .unwrap()
        .starts_with("nextval("));
    let dept = cols.iter().find(|c| c["name"] == "department_id").unwrap();
    assert_eq!(dept["foreign_key"]["table"], "departments");
    assert_eq!(dept["foreign_key"]["column"], "id");
    let salary = cols.iter().find(|c| c["name"] == "salary").unwrap();
    assert_eq!(salary["data_type"], "numeric(12,2)");
    assert_eq!(salary["nullable"], false);
    let manager = cols.iter().find(|c| c["name"] == "manager_id").unwrap();
    assert_eq!(manager["nullable"], true);
    assert_eq!(
        manager["foreign_key"]["table"], "employees",
        "self reference"
    );

    // Non-public schema via ?schema=.
    let (status, cols) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/tables/orders/columns?schema=sales"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let customer = cols
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "customer_id")
        .unwrap();
    assert_eq!(customer["foreign_key"]["schema"], "sales");
    assert_eq!(customer["foreign_key"]["table"], "customers");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn relationships_and_indexes(pool: PgPool) {
    let (app, token, id) = setup(pool).await;
    let (status, rels) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/tables/employees/relationships"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{rels}");
    let rels = rels.as_array().unwrap();
    let outgoing: Vec<_> = rels
        .iter()
        .filter(|r| r["direction"] == "outgoing")
        .collect();
    let incoming: Vec<_> = rels
        .iter()
        .filter(|r| r["direction"] == "incoming")
        .collect();
    assert!(outgoing
        .iter()
        .any(|r| r["to_table"] == "departments" && r["from_columns"][0] == "department_id"));
    assert!(incoming
        .iter()
        .any(|r| r["from_schema"] == "sales" && r["from_table"] == "orders"));

    let (status, idx) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/tables/orders/indexes?schema=sales"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let idx = idx.as_array().unwrap();
    assert_eq!(idx[0]["is_primary"], true, "primary index first");
    assert!(idx.iter().any(|i| i["name"] == "orders_customer_idx"));
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn catalog_and_errors(pool: PgPool) {
    let (app, token, id) = setup(pool).await;
    let (status, catalog) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/catalog"),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let orders = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["schema"] == "sales" && e["table"] == "orders")
        .unwrap();
    assert_eq!(orders["columns"][0], "id");
    assert_eq!(orders["column_types"][0], "bigint");

    // Unknown table -> 404; hostile names are just names (parameterised).
    for name in ["nope", "employees; DROP TABLE x", "employees' OR '1'='1"] {
        let path = format!(
            "/api/connections/{id}/tables/{}/columns",
            name.replace(' ', "%20")
                .replace(';', "%3B")
                .replace('\'', "%27")
        );
        let (status, _) = send(&app, api("GET", &path, None, Some(&token))).await;
        assert_eq!(status, 404, "{name}");
    }

    // Another user cannot introspect this connection.
    let other = register(&app, "b@example.com").await;
    let (status, _) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{id}/schemas"),
            None,
            Some(&other),
        ),
    )
    .await;
    assert_eq!(status, 404);
}

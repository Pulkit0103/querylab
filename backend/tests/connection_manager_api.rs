//! Connection manager: groups, environments, colours, custom CA bundles,
//! duplication and connection status.

mod common;

use common::{api, register, sample_connection, send};
use serde_json::{json, Value};
use sqlx::PgPool;

const CERT: &str =
    "-----BEGIN CERTIFICATE-----\nMIIBszCCAVmgAwIBAgIUQ1Fq\n-----END CERTIFICATE-----";

async fn create(app: &axum::Router, token: &str, body: Value) -> (u16, Value) {
    let (status, json) = send(
        app,
        api("POST", "/api/connections", Some(body), Some(token)),
    )
    .await;
    (status.as_u16(), json)
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn groups_are_owned_and_organise_connections(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let alice = register(&app, "alice@example.com").await;
    let mallory = register(&app, "mallory@example.com").await;

    let (status, staging) = send(
        &app,
        api(
            "POST",
            "/api/connection-groups",
            Some(json!({ "name": "Staging DBs" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 201, "{staging}");
    let group_id = staging["id"].as_str().unwrap().to_string();
    assert_eq!(staging["position"], 0);
    let (_, second) = send(
        &app,
        api(
            "POST",
            "/api/connection-groups",
            Some(json!({ "name": "Analytics" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(second["position"], 1, "appended after existing groups");

    // Names are unique per user, case-insensitively.
    let (status, _) = send(
        &app,
        api(
            "POST",
            "/api/connection-groups",
            Some(json!({ "name": "staging dbs" })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 409);

    // File a connection under the group.
    let mut body = sample_connection("Grouped");
    body["group_id"] = json!(group_id);
    body["environment"] = json!("staging");
    body["color"] = json!("#3B82F6");
    let (status, conn) = create(&app, &alice, body).await;
    assert_eq!(status, 201, "{conn}");
    assert_eq!(conn["group_id"], group_id.as_str());
    assert_eq!(conn["environment"], "staging");
    assert_eq!(conn["color"], "#3b82f6", "stored lowercase");
    assert!(conn["last_status"].is_null());

    let (_, groups) = send(
        &app,
        api("GET", "/api/connection-groups", None, Some(&alice)),
    )
    .await;
    assert_eq!(groups[0]["name"], "Staging DBs");
    assert_eq!(groups[0]["connection_count"], 1);

    // Another user can neither see, change, delete nor reference the group.
    let (_, theirs) = send(
        &app,
        api("GET", "/api/connection-groups", None, Some(&mallory)),
    )
    .await;
    assert!(theirs.as_array().unwrap().is_empty());
    let path = format!("/api/connection-groups/{group_id}");
    assert_eq!(
        send(
            &app,
            api("PUT", &path, Some(json!({ "name": "x" })), Some(&mallory))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        send(&app, api("DELETE", &path, None, Some(&mallory)))
            .await
            .0,
        404
    );
    let mut stolen = sample_connection("Mallory");
    stolen["group_id"] = json!(group_id);
    let (status, res) = create(&app, &mallory, stolen).await;
    assert_eq!(status, 404, "{res}");

    // Rename and reorder.
    let (status, renamed) = send(
        &app,
        api(
            "PUT",
            &path,
            Some(json!({ "name": "Staging", "position": 5 })),
            Some(&alice),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(renamed["name"], "Staging");
    assert_eq!(renamed["position"], 5);

    // Deleting the group keeps the connection, ungrouped.
    assert_eq!(
        send(&app, api("DELETE", &path, None, Some(&alice))).await.0,
        204
    );
    let (_, after) = send(
        &app,
        api(
            "GET",
            &format!("/api/connections/{}", conn["id"].as_str().unwrap()),
            None,
            Some(&alice),
        ),
    )
    .await;
    assert!(after["group_id"].is_null());
    assert_eq!(after["environment"], "staging");
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn new_fields_are_validated_and_can_be_cleared(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;

    for (field, value) in [
        ("color", json!("blue")),
        ("environment", json!("prod")),
        (
            "ssl_root_cert",
            json!("-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY-----"),
        ),
    ] {
        let mut body = sample_connection("Bad");
        body[field] = value;
        let (status, res) = create(&app, &token, body).await;
        assert!(status == 422 || status == 400, "{field}: {status} {res}");
    }

    let mut body = sample_connection("With CA");
    body["ssl_root_cert"] = json!(CERT);
    body["color"] = json!("#10b981");
    body["environment"] = json!("production");
    let (status, conn) = create(&app, &token, body).await;
    assert_eq!(status, 201, "{conn}");
    assert!(conn["ssl_root_cert"]
        .as_str()
        .unwrap()
        .starts_with("-----BEGIN CERTIFICATE-----"));
    assert_eq!(conn["environment"], "production");

    // Explicit null clears; absent keeps.
    let path = format!("/api/connections/{}", conn["id"].as_str().unwrap());
    let (status, updated) = send(
        &app,
        api(
            "PUT",
            &path,
            Some(json!({ "ssl_root_cert": null, "name": "Renamed" })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert!(updated["ssl_root_cert"].is_null());
    assert_eq!(updated["color"], "#10b981");
    assert_eq!(updated["environment"], "production");
    let (_, updated) = send(
        &app,
        api("PUT", &path, Some(json!({ "color": null })), Some(&token)),
    )
    .await;
    assert!(updated["color"].is_null());
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn duplicate_copies_settings_and_working_credentials(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let mut body = sample_connection("Sample");
    body["environment"] = json!("staging");
    body["color"] = json!("#f59e0b");
    let (_, original) = create(&app, &token, body).await;
    let id = original["id"].as_str().unwrap();

    let dup = |id: String| {
        api(
            "POST",
            &format!("/api/connections/{id}/duplicate"),
            None,
            Some(&token),
        )
    };
    let (status, copy) = send(&app, dup(id.to_string())).await;
    assert_eq!(status, 201, "{copy}");
    assert_ne!(copy["id"], original["id"]);
    assert_eq!(copy["name"], "Sample (copy)");
    assert_eq!(copy["environment"], "staging");
    assert_eq!(copy["color"], "#f59e0b");
    assert_eq!(copy["host"], original["host"]);
    let (_, copy2) = send(&app, dup(id.to_string())).await;
    assert_eq!(copy2["name"], "Sample (copy 2)");

    // The copied secret decrypts under the new id and connects.
    let (status, tested) = send(
        &app,
        api(
            "POST",
            &format!("/api/connections/{}/test", copy["id"].as_str().unwrap()),
            None,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, 200, "{tested}");

    // Other users cannot duplicate it.
    let other = register(&app, "b@example.com").await;
    let (status, _) = send(
        &app,
        api(
            "POST",
            &format!("/api/connections/{id}/duplicate"),
            None,
            Some(&other),
        ),
    )
    .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn testing_records_connection_status(pool: PgPool) {
    let (app, _) = common::app_with_pool(pool);
    let token = register(&app, "a@example.com").await;
    let get = |id: &str| api("GET", &format!("/api/connections/{id}"), None, Some(&token));
    let test = |id: &str| {
        api(
            "POST",
            &format!("/api/connections/{id}/test"),
            None,
            Some(&token),
        )
    };

    // Online, with latency and last_connected_at.
    let (_, good) = create(&app, &token, sample_connection("Good")).await;
    let good_id = good["id"].as_str().unwrap();
    assert_eq!(send(&app, test(good_id)).await.0, 200);
    let (_, rec) = send(&app, get(good_id)).await;
    assert_eq!(rec["last_status"], "online");
    assert!(rec["last_latency_ms"].as_i64().unwrap() >= 0);
    assert!(rec["last_status_at"].is_string());
    assert!(rec["last_connected_at"].is_string());
    assert!(rec["last_error"].is_null());

    // Wrong password: auth_failed, with the server's message.
    let mut bad = sample_connection("Bad password");
    bad["password"] = json!("definitely-wrong");
    let (_, bad) = create(&app, &token, bad).await;
    let bad_id = bad["id"].as_str().unwrap();
    let (status, err) = send(&app, test(bad_id)).await;
    assert_eq!(status, 502, "{err}");
    let (_, rec) = send(&app, get(bad_id)).await;
    assert_eq!(rec["last_status"], "auth_failed", "{rec}");
    assert!(rec["last_error"].as_str().unwrap().contains("password"));
    assert!(rec["last_latency_ms"].is_null());

    // Nothing listening: unreachable.
    let mut down = sample_connection("Down");
    // Ports below 1024 are refused outright (SSRF rules); use a closed high port.
    down["port"] = json!(59_999);
    let (_, down) = create(&app, &token, down).await;
    let down_id = down["id"].as_str().unwrap();
    assert_eq!(send(&app, test(down_id)).await.0, 502);
    assert_eq!(
        send(&app, get(down_id)).await.1["last_status"],
        "unreachable"
    );

    // verify-full against a server without TLS: tls_error.
    let mut tls = sample_connection("TLS");
    tls["ssl_mode"] = json!("verify-full");
    tls["ssl_root_cert"] = json!(CERT);
    let (_, tls) = create(&app, &token, tls).await;
    let tls_id = tls["id"].as_str().unwrap();
    assert_eq!(send(&app, test(tls_id)).await.0, 502);
    let status = send(&app, get(tls_id)).await.1["last_status"].clone();
    assert!(status == "tls_error" || status == "error", "{status}");

    // Changing the target clears the stale status; renaming does not.
    let path = format!("/api/connections/{good_id}");
    let (_, renamed) = send(
        &app,
        api(
            "PUT",
            &path,
            Some(json!({ "name": "Good 2" })),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(renamed["last_status"], "online");
    let (_, moved) = send(
        &app,
        api("PUT", &path, Some(json!({ "port": 5433 })), Some(&token)),
    )
    .await;
    assert!(moved["last_status"].is_null(), "{moved}");
}

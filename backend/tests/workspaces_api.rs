//! Phase 10b: team workspaces, roles, connection grants, invitations,
//! shared saved queries and snippets — a permission matrix over the API.

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

struct Team {
    app: Router,
    pool: PgPool,
    ws: String,
    owner: String,
    admin: String,
    editor: String,
    viewer: String,
    outsider: String,
}

/// Creates a team workspace with an owner, an admin, an editor and a viewer
/// (each joining through an invitation), plus a user outside it.
async fn team(pool: PgPool) -> Team {
    let (app, _) = common::app_with_pool(pool.clone());
    let owner = register(&app, "owner@example.com").await;
    let (status, ws) = call(
        &app,
        &owner,
        "POST",
        "/api/workspaces",
        Some(json!({ "name": "Data team" })),
    )
    .await;
    assert_eq!(status, 201, "{ws}");
    assert_eq!(ws["role"], "owner");
    let id = ws["id"].as_str().unwrap().to_string();
    let mut tokens = Vec::new();
    for (email, role) in [
        ("admin@example.com", "admin"),
        ("editor@example.com", "editor"),
        ("viewer@example.com", "viewer"),
    ] {
        let (status, inv) = call(
            &app,
            &owner,
            "POST",
            &format!("/api/workspaces/{id}/invitations"),
            Some(json!({ "email": email, "role": role })),
        )
        .await;
        assert_eq!(status, 201, "{inv}");
        let link = inv["link"].as_str().unwrap();
        let token = link.split("token=").nth(1).unwrap().to_string();
        let user = register(&app, email).await;
        let (status, joined) = call(
            &app,
            &user,
            "POST",
            &format!("/api/invitations/{token}/accept"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{joined}");
        assert_eq!(joined["role"], role);
        tokens.push(user);
    }
    let outsider = register(&app, "outsider@example.com").await;
    let mut t = tokens.into_iter();
    Team {
        app,
        pool,
        ws: id,
        owner,
        admin: t.next().unwrap(),
        editor: t.next().unwrap(),
        viewer: t.next().unwrap(),
        outsider,
    }
}

fn owner_connection(name: &str, ws: &str) -> Value {
    let mut body = sample_connection(name);
    body["username"] = json!("sample_owner");
    body["password"] = json!("sample_owner");
    body["read_only"] = json!(false);
    body["workspace_id"] = json!(ws);
    body
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL and the sample database"]
async fn connection_permission_matrix(pool: PgPool) {
    let t = team(pool).await;
    let app = &t.app;
    // Viewers cannot create connections; editors can.
    let (status, _) = call(
        app,
        &t.viewer,
        "POST",
        "/api/connections",
        Some(owner_connection("Viewer's", &t.ws)),
    )
    .await;
    assert_eq!(status, 403);
    let (status, conn) = call(
        app,
        &t.owner,
        "POST",
        "/api/connections",
        Some(owner_connection("Shared RW", &t.ws)),
    )
    .await;
    assert_eq!(status, 201, "{conn}");
    let id = conn["id"].as_str().unwrap().to_string();
    assert_eq!(conn["workspace_id"], t.ws.as_str());

    // An editor's own connection is theirs to manage.
    let (status, own) = call(
        app,
        &t.editor,
        "POST",
        "/api/connections",
        Some(owner_connection("Editor's", &t.ws)),
    )
    .await;
    assert_eq!(status, 201, "{own}");
    assert_eq!(own["access"], "admin");

    // Everyone in the workspace sees it, with access matching their role.
    for (token, access) in [
        (&t.owner, "admin"),
        (&t.admin, "admin"),
        (&t.editor, "write"),
        (&t.viewer, "read"),
    ] {
        let (status, list) = call(
            app,
            token,
            "GET",
            &format!("/api/connections?workspace_id={}", t.ws),
            None,
        )
        .await;
        assert_eq!(status, 200);
        let found = list
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id.as_str())
            .expect("visible");
        assert_eq!(found["access"], access, "{found}");
        // Members never receive credentials.
        assert!(
            found.get("password").is_none() && !found.to_string().contains("encrypted"),
            "{found}"
        );
    }
    // Outsiders: 404 everywhere, never 403.
    for (method, path) in [
        ("GET", format!("/api/connections/{id}")),
        ("PUT", format!("/api/connections/{id}")),
        ("DELETE", format!("/api/connections/{id}")),
        ("GET", format!("/api/connections/{id}/schemas")),
        ("GET", format!("/api/connections/{id}/grants")),
        ("GET", format!("/api/workspaces/{}", t.ws)),
        ("GET", format!("/api/workspaces/{}/members", t.ws)),
    ] {
        let body = (method == "PUT").then(|| json!({ "name": "pwned" }));
        let (status, _) = call(app, &t.outsider, method, &path, body).await;
        assert_eq!(status, 404, "{method} {path}");
    }
    let (_, list) = call(app, &t.outsider, "GET", "/api/connections", None).await;
    assert!(list.as_array().unwrap().is_empty());

    // Managing the connection needs admin access.
    for (token, status) in [
        (&t.viewer, 403),
        (&t.editor, 403),
        (&t.admin, 200),
        (&t.owner, 200),
    ] {
        let (s, body) = call(
            app,
            token,
            "PUT",
            &format!("/api/connections/{id}"),
            Some(json!({ "environment": "development" })),
        )
        .await;
        assert_eq!(s, status, "{body}");
    }

    // Writes: editors may write, viewers run read-only even on a read-write
    // connection and are refused privileged operations.
    let exec = |sql: &str| json!({ "connection_id": id, "query": sql });
    let (status, _) = call(
        app,
        &t.editor,
        "POST",
        "/api/query/execute",
        Some(exec("CREATE TEMP TABLE scratch (i int)")),
    )
    .await;
    assert_eq!(status, 200);
    let (status, err) = call(
        app,
        &t.viewer,
        "POST",
        "/api/query/execute",
        Some(exec("CREATE TEMP TABLE scratch (i int)")),
    )
    .await;
    assert_eq!(status, 400, "{err}");
    assert_eq!(err["error"]["details"]["sqlstate"], "25006");
    let (status, _) = call(
        app,
        &t.viewer,
        "POST",
        "/api/query/execute",
        Some(exec("SELECT 1")),
    )
    .await;
    assert_eq!(status, 200);
    let apply = json!({ "schema": "public", "table": "does_not_exist", "changes": { "inserts": [{ "values": { "a": 1 } }] } });
    let (status, _) = call(
        app,
        &t.viewer,
        "POST",
        &format!("/api/connections/{id}/data/apply"),
        Some(apply.clone()),
    )
    .await;
    assert_eq!(status, 403);
    let (status, _) = call(
        app,
        &t.editor,
        "POST",
        &format!("/api/connections/{id}/data/apply"),
        Some(apply),
    )
    .await;
    assert_eq!(status, 404, "authorized, then the table is missing");
    let mock = json!({ "tables": [{ "schema": "public", "table": "departments", "rows": 1 }] });
    let (status, _) = call(
        app,
        &t.viewer,
        "POST",
        &format!("/api/connections/{id}/mock-data/run"),
        Some(mock),
    )
    .await;
    assert_eq!(status, 403);
    let (status, _) = call(
        app,
        &t.viewer,
        "POST",
        &format!("/api/connections/{id}/monitor/backends/1/cancel"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 403);
    // Reads work for viewers.
    let (status, _) = call(
        app,
        &t.viewer,
        "GET",
        &format!("/api/connections/{id}/schemas"),
        None,
    )
    .await;
    assert_eq!(status, 200);

    // Grants: only connection admins see and change them.
    assert_eq!(
        call(
            app,
            &t.editor,
            "GET",
            &format!("/api/connections/{id}/grants"),
            None
        )
        .await
        .0,
        403
    );
    let (status, grants) = call(
        app,
        &t.owner,
        "GET",
        &format!("/api/connections/{id}/grants"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(grants.as_array().unwrap().len(), 4);
    assert!(!grants.to_string().contains("password"));
    let member = |email: &str| {
        grants
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["email"] == email)
            .unwrap()["user_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (editor_id, viewer_id, admin_id) = (
        member("editor@example.com"),
        member("viewer@example.com"),
        member("admin@example.com"),
    );
    // Viewers cannot be given write; admins always manage.
    let (status, _) = call(
        app,
        &t.owner,
        "PUT",
        &format!("/api/connections/{id}/grants/{viewer_id}"),
        Some(json!({ "access": "write" })),
    )
    .await;
    assert_eq!(status, 422);
    let (status, _) = call(
        app,
        &t.owner,
        "PUT",
        &format!("/api/connections/{id}/grants/{admin_id}"),
        Some(json!({ "access": "read" })),
    )
    .await;
    assert_eq!(status, 422);
    // Downgrade the editor to read: writes are refused, execution read-only.
    let (status, _) = call(
        app,
        &t.owner,
        "PUT",
        &format!("/api/connections/{id}/grants/{editor_id}"),
        Some(json!({ "access": "read" })),
    )
    .await;
    assert_eq!(status, 200);
    let (status, err) = call(
        app,
        &t.editor,
        "POST",
        "/api/query/execute",
        Some(exec("CREATE TEMP TABLE scratch (i int)")),
    )
    .await;
    assert_eq!(
        (status, err["error"]["details"]["sqlstate"].as_str()),
        (400, Some("25006"))
    );
    // Hide it from the editor entirely.
    call(
        app,
        &t.owner,
        "PUT",
        &format!("/api/connections/{id}/grants/{editor_id}"),
        Some(json!({ "access": "none" })),
    )
    .await;
    assert_eq!(
        call(
            app,
            &t.editor,
            "GET",
            &format!("/api/connections/{id}"),
            None
        )
        .await
        .0,
        404
    );
    // Back to the role default.
    call(
        app,
        &t.owner,
        "PUT",
        &format!("/api/connections/{id}/grants/{editor_id}"),
        Some(json!({ "access": null })),
    )
    .await;
    let (_, c) = call(
        app,
        &t.editor,
        "GET",
        &format!("/api/connections/{id}"),
        None,
    )
    .await;
    assert_eq!(c["access"], "write");

    // History of a shared connection is per member.
    let (_, history) = call(app, &t.viewer, "GET", "/api/history", None).await;
    assert!(history["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(
            |e| e["sql_content"] != "CREATE TEMP TABLE scratch (i int)" || e["status"] != "success"
        ));

    // Permission changes are audited and visible to workspace admins.
    let (_, events) = call(
        app,
        &t.admin,
        "GET",
        "/api/audit-events?action=permission.",
        None,
    )
    .await;
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "permission.grant_changed"),
        "{events}"
    );
    let (_, events) = call(
        app,
        &t.viewer,
        "GET",
        "/api/audit-events?action=permission.",
        None,
    )
    .await;
    assert!(events["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["action"] != "permission.grant_changed"));

    // Deleting needs admin access.
    assert_eq!(
        call(
            app,
            &t.editor,
            "DELETE",
            &format!("/api/connections/{id}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.owner,
            "DELETE",
            &format!("/api/connections/{id}"),
            None
        )
        .await
        .0,
        204
    );
    let _ = &t.pool;
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn membership_and_invitations(pool: PgPool) {
    let t = team(pool).await;
    let app = &t.app;
    let ws = &t.ws;
    let (_, members) = call(
        app,
        &t.viewer,
        "GET",
        &format!("/api/workspaces/{ws}/members"),
        None,
    )
    .await;
    assert_eq!(members.as_array().unwrap().len(), 4);
    let id_of = |email: &str| {
        members
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["email"] == email)
            .unwrap()["user_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (owner_id, admin_id, editor_id, viewer_id) = (
        id_of("owner@example.com"),
        id_of("admin@example.com"),
        id_of("editor@example.com"),
        id_of("viewer@example.com"),
    );

    // Who may invite and change roles.
    let invite = |role: &str| Some(json!({ "email": "new@example.com", "role": role }));
    assert_eq!(
        call(
            app,
            &t.viewer,
            "POST",
            &format!("/api/workspaces/{ws}/invitations"),
            invite("viewer")
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.editor,
            "POST",
            &format!("/api/workspaces/{ws}/invitations"),
            invite("viewer")
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.admin,
            "POST",
            &format!("/api/workspaces/{ws}/invitations"),
            invite("owner")
        )
        .await
        .0,
        422
    );
    assert_eq!(
        call(
            app,
            &t.editor,
            "PUT",
            &format!("/api/workspaces/{ws}/members/{viewer_id}"),
            Some(json!({ "role": "editor" }))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.admin,
            "PUT",
            &format!("/api/workspaces/{ws}/members/{editor_id}"),
            Some(json!({ "role": "owner" }))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.admin,
            "PUT",
            &format!("/api/workspaces/{ws}/members/{owner_id}"),
            Some(json!({ "role": "viewer" }))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.admin,
            "PUT",
            &format!("/api/workspaces/{ws}/members/{viewer_id}"),
            Some(json!({ "role": "editor" }))
        )
        .await
        .0,
        200
    );
    // The last owner cannot step down or leave.
    assert_eq!(
        call(
            app,
            &t.owner,
            "PUT",
            &format!("/api/workspaces/{ws}/members/{owner_id}"),
            Some(json!({ "role": "admin" }))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            app,
            &t.owner,
            "DELETE",
            &format!("/api/workspaces/{ws}/members/{owner_id}"),
            None
        )
        .await
        .0,
        409
    );
    // Owners promote; then the first owner may leave.
    assert_eq!(
        call(
            app,
            &t.owner,
            "PUT",
            &format!("/api/workspaces/{ws}/members/{admin_id}"),
            Some(json!({ "role": "owner" }))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            app,
            &t.owner,
            "DELETE",
            &format!("/api/workspaces/{ws}/members/{owner_id}"),
            None
        )
        .await
        .0,
        204
    );
    assert_eq!(
        call(app, &t.owner, "GET", &format!("/api/workspaces/{ws}"), None)
            .await
            .0,
        404
    );
    // Editors cannot remove others; anyone can leave.
    assert_eq!(
        call(
            app,
            &t.editor,
            "DELETE",
            &format!("/api/workspaces/{ws}/members/{viewer_id}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.editor,
            "DELETE",
            &format!("/api/workspaces/{ws}/members/{editor_id}"),
            None
        )
        .await
        .0,
        204
    );

    // Invitations: email-bound, single use, revocable, expiring.
    let (status, inv) = call(
        app,
        &t.admin,
        "POST",
        &format!("/api/workspaces/{ws}/invitations"),
        invite("viewer"),
    )
    .await;
    assert_eq!(status, 201, "{inv}");
    let token = inv["link"]
        .as_str()
        .unwrap()
        .split("token=")
        .nth(1)
        .unwrap()
        .to_string();
    let (status, preview) = call(
        app,
        &t.outsider,
        "GET",
        &format!("/api/invitations/{token}"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(preview["workspace_name"], "Data team");
    // Wrong account.
    assert_eq!(
        call(
            app,
            &t.outsider,
            "POST",
            &format!("/api/invitations/{token}/accept"),
            None
        )
        .await
        .0,
        403
    );
    let newcomer = register(app, "new@example.com").await;
    assert_eq!(
        call(
            app,
            &newcomer,
            "POST",
            &format!("/api/invitations/{token}/accept"),
            None
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            app,
            &newcomer,
            "POST",
            &format!("/api/invitations/{token}/accept"),
            None
        )
        .await
        .0,
        404
    );
    let (_, pending) = call(
        app,
        &t.admin,
        "GET",
        &format!("/api/workspaces/{ws}/invitations"),
        None,
    )
    .await;
    assert!(pending.as_array().unwrap().is_empty());

    let (_, inv) = call(
        app,
        &t.admin,
        "POST",
        &format!("/api/workspaces/{ws}/invitations"),
        Some(json!({ "email": "late@example.com", "role": "editor" })),
    )
    .await;
    let late_token = inv["link"]
        .as_str()
        .unwrap()
        .split("token=")
        .nth(1)
        .unwrap()
        .to_string();
    let inv_id = inv["invitation"]["id"].as_str().unwrap();
    assert_eq!(
        call(
            app,
            &t.admin,
            "DELETE",
            &format!("/api/workspaces/{ws}/invitations/{inv_id}"),
            None
        )
        .await
        .0,
        204
    );
    let late = register(app, "late@example.com").await;
    assert_eq!(
        call(
            app,
            &late,
            "POST",
            &format!("/api/invitations/{late_token}/accept"),
            None
        )
        .await
        .0,
        404
    );
    let (_, inv) = call(
        app,
        &t.admin,
        "POST",
        &format!("/api/workspaces/{ws}/invitations"),
        Some(json!({ "email": "late@example.com", "role": "editor" })),
    )
    .await;
    let expired = inv["link"]
        .as_str()
        .unwrap()
        .split("token=")
        .nth(1)
        .unwrap()
        .to_string();
    sqlx::query("UPDATE workspace_invitations SET expires_at = now() - interval '1 minute' WHERE accepted_at IS NULL AND revoked_at IS NULL")
        .execute(&t.pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            app,
            &late,
            "POST",
            &format!("/api/invitations/{expired}/accept"),
            None
        )
        .await
        .0,
        404
    );

    // Personal workspaces cannot be shared or deleted.
    let (_, list) = call(app, &t.outsider, "GET", "/api/workspaces", None).await;
    let personal = list
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["personal"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        call(
            app,
            &t.outsider,
            "POST",
            &format!("/api/workspaces/{personal}/invitations"),
            invite("viewer")
        )
        .await
        .0,
        400
    );
    assert_eq!(
        call(
            app,
            &t.outsider,
            "DELETE",
            &format!("/api/workspaces/{personal}"),
            None
        )
        .await
        .0,
        400
    );
    // Only owners delete a team workspace.
    assert_eq!(
        call(
            app,
            &t.viewer,
            "DELETE",
            &format!("/api/workspaces/{ws}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.admin,
            "DELETE",
            &format!("/api/workspaces/{ws}"),
            None
        )
        .await
        .0,
        204
    );
}

#[sqlx::test(migrator = "querylab_api::database::MIGRATOR")]
#[ignore = "requires DATABASE_URL"]
async fn shared_queries_and_snippets(pool: PgPool) {
    let t = team(pool).await;
    let app = &t.app;
    let ws = &t.ws;

    // Saved queries and folders: editors write, viewers read, outsiders see nothing.
    let (status, folder) = call(
        app,
        &t.editor,
        "POST",
        "/api/collections",
        Some(json!({ "name": "Reports", "workspace_id": ws })),
    )
    .await;
    assert_eq!(status, 201, "{folder}");
    let (status, saved) = call(app, &t.editor, "POST", "/api/queries", Some(json!({
        "workspace_id": ws, "name": "Revenue", "sql_content": "select 1", "collection_id": folder["id"],
    }))).await;
    assert_eq!(status, 201, "{saved}");
    let qid = saved["id"].as_str().unwrap().to_string();
    assert_eq!(saved["workspace_id"], ws.as_str());
    let (_, list) = call(
        app,
        &t.viewer,
        "GET",
        &format!("/api/queries?workspace_id={ws}"),
        None,
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(
        call(app, &t.viewer, "GET", &format!("/api/queries/{qid}"), None)
            .await
            .0,
        200
    );
    assert_eq!(
        call(
            app,
            &t.viewer,
            "PUT",
            &format!("/api/queries/{qid}"),
            Some(json!({ "name": "x" }))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.viewer,
            "POST",
            "/api/queries",
            Some(json!({ "workspace_id": ws, "name": "v", "sql_content": "select 2" }))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.viewer,
            "DELETE",
            &format!("/api/queries/{qid}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.owner,
            "PUT",
            &format!("/api/queries/{qid}"),
            Some(json!({ "name": "Revenue v2" }))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            app,
            &t.outsider,
            "GET",
            &format!("/api/queries/{qid}"),
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            app,
            &t.outsider,
            "GET",
            &format!("/api/queries?workspace_id={ws}"),
            None
        )
        .await
        .0,
        404
    );
    // The personal library stays separate.
    let (_, mine) = call(app, &t.editor, "GET", "/api/queries", None).await;
    assert!(mine.as_array().unwrap().is_empty());

    // Snippets.
    let snippet = json!({ "workspace_id": ws, "name": "Top N", "body": "select * from {{table}} limit {{n}}", "tags": ["util"] });
    assert_eq!(
        call(
            app,
            &t.viewer,
            "POST",
            "/api/snippets",
            Some(snippet.clone())
        )
        .await
        .0,
        403
    );
    let (status, created) = call(
        app,
        &t.editor,
        "POST",
        "/api/snippets",
        Some(snippet.clone()),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["created_by_email"], "editor@example.com");
    assert_eq!(
        call(app, &t.editor, "POST", "/api/snippets", Some(snippet))
            .await
            .0,
        409
    );
    let sid = created["id"].as_str().unwrap();
    let (_, list) = call(
        app,
        &t.viewer,
        "GET",
        &format!("/api/snippets?workspace_id={ws}"),
        None,
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(
        call(
            app,
            &t.outsider,
            "GET",
            &format!("/api/snippets/{sid}"),
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            app,
            &t.viewer,
            "DELETE",
            &format!("/api/snippets/{sid}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            app,
            &t.admin,
            "DELETE",
            &format!("/api/snippets/{sid}"),
            None
        )
        .await
        .0,
        204
    );
}

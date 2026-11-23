//! Workspaces, members, invitations, connection grants and snippets
//! (metadata database). Authorization happens in `services::authz`; these
//! functions assume the caller has been checked.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::workspaces::{
    GrantRow, Invitation, InvitationPreview, Member, Snippet, WorkspaceSummary,
};

pub async fn personal_workspace(db: &PgPool, user_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM workspaces WHERE personal AND created_by = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await
}

/// Creates the personal workspace of a user (idempotent).
pub async fn ensure_personal(db: &PgPool, user_id: Uuid) -> Result<Uuid, sqlx::Error> {
    if let Some(id) = personal_workspace(db, user_id).await? {
        return Ok(id);
    }
    let mut tx = db.begin().await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, personal, created_by) VALUES ($1, 'Personal', true, $2) \
         ON CONFLICT (created_by) WHERE personal DO NOTHING",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    let id: Uuid =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE personal AND created_by = $1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'owner') \
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// The user's role in a workspace, if a member.
pub async fn role(
    db: &PgPool,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<Option<(String, bool)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.role, w.personal FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.workspace_id = $1 AND m.user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Workspaces where the user has at least one of `roles`.
pub async fn workspaces_with_roles(
    db: &PgPool,
    user_id: Uuid,
    roles: &[&str],
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT workspace_id FROM workspace_members WHERE user_id = $1 AND role = ANY($2)",
    )
    .bind(user_id)
    .bind(roles)
    .fetch_all(db)
    .await
}

pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<WorkspaceSummary>, sqlx::Error> {
    sqlx::query_as(
        "SELECT w.id, w.name, w.personal, m.role, \
                (SELECT count(*) FROM workspace_members x WHERE x.workspace_id = w.id) AS member_count, \
                w.created_at \
         FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.user_id = $1 ORDER BY w.personal DESC, lower(w.name), w.id",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn get_for_user(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<WorkspaceSummary>, sqlx::Error> {
    sqlx::query_as(
        "SELECT w.id, w.name, w.personal, m.role, \
                (SELECT count(*) FROM workspace_members x WHERE x.workspace_id = w.id) AS member_count, \
                w.created_at \
         FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id \
         WHERE m.user_id = $1 AND w.id = $2",
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await
}

pub async fn create(db: &PgPool, user_id: Uuid, name: &str) -> Result<Uuid, sqlx::Error> {
    let mut tx = db.begin().await?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, personal, created_by) VALUES ($1, $2, false, $3)",
    )
    .bind(id)
    .bind(name)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn rename(db: &PgPool, id: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE workspaces SET name = $2 WHERE id = $1")
        .bind(id)
        .bind(name)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM workspaces WHERE id = $1 AND NOT personal")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Members
// ---------------------------------------------------------------------------

pub async fn members(db: &PgPool, workspace_id: Uuid) -> Result<Vec<Member>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.user_id, u.email, u.display_name, m.role, m.joined_at \
         FROM workspace_members m JOIN users u ON u.id = m.user_id \
         WHERE m.workspace_id = $1 \
         ORDER BY CASE m.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 WHEN 'editor' THEN 2 ELSE 3 END, lower(u.email)",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
}

pub async fn owner_count(db: &PgPool, workspace_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM workspace_members WHERE workspace_id = $1 AND role = 'owner'",
    )
    .bind(workspace_id)
    .fetch_one(db)
    .await
}

pub async fn set_role(
    db: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(role)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Removes a member and their grants on the workspace's connections.
pub async fn remove_member(
    db: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "DELETE FROM connection_grants g USING database_connections c \
         WHERE g.connection_id = c.id AND c.workspace_id = $1 AND g.user_id = $2",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    let done =
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
            .bind(workspace_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(done.rows_affected() == 1)
}

// ---------------------------------------------------------------------------
// Invitations
// ---------------------------------------------------------------------------

const INVITATION_SELECT: &str =
    "SELECT i.id, i.workspace_id, i.email, i.role, u.email AS invited_by_email, \
            i.created_at, i.expires_at \
     FROM workspace_invitations i LEFT JOIN users u ON u.id = i.invited_by";

pub async fn insert_invitation(
    db: &PgPool,
    workspace_id: Uuid,
    email: &str,
    role: &str,
    token_hash: &[u8],
    invited_by: Uuid,
    expires_at: DateTime<Utc>,
) -> Result<Invitation, sqlx::Error> {
    let id = Uuid::new_v4();
    // A new invitation replaces any pending one for the same address.
    sqlx::query(
        "UPDATE workspace_invitations SET revoked_at = now() \
         WHERE workspace_id = $1 AND lower(email) = lower($2) AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(workspace_id)
    .bind(email)
    .execute(db)
    .await?;
    sqlx::query(
        "INSERT INTO workspace_invitations (id, workspace_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(email)
    .bind(role)
    .bind(token_hash)
    .bind(invited_by)
    .bind(expires_at)
    .execute(db)
    .await?;
    sqlx::query_as(&format!("{INVITATION_SELECT} WHERE i.id = $1"))
        .bind(id)
        .fetch_one(db)
        .await
}

pub async fn pending_invitations(
    db: &PgPool,
    workspace_id: Uuid,
) -> Result<Vec<Invitation>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{INVITATION_SELECT} WHERE i.workspace_id = $1 AND i.accepted_at IS NULL \
         AND i.revoked_at IS NULL AND i.expires_at > now() ORDER BY i.created_at DESC"
    ))
    .bind(workspace_id)
    .fetch_all(db)
    .await
}

pub async fn revoke_invitation(
    db: &PgPool,
    workspace_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE workspace_invitations SET revoked_at = now() \
         WHERE id = $1 AND workspace_id = $2 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(workspace_id)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn preview_invitation(
    db: &PgPool,
    token_hash: &[u8],
) -> Result<Option<InvitationPreview>, sqlx::Error> {
    sqlx::query_as(
        "SELECT w.name AS workspace_name, i.email, i.role, u.email AS invited_by_email, i.expires_at \
         FROM workspace_invitations i JOIN workspaces w ON w.id = i.workspace_id \
         LEFT JOIN users u ON u.id = i.invited_by \
         WHERE i.token_hash = $1 AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await
}

/// Accepts an invitation addressed to `email`: marks it used and adds the
/// member (an existing membership keeps the higher role). Returns the
/// workspace and role.
pub async fn accept_invitation(
    db: &PgPool,
    token_hash: &[u8],
    user_id: Uuid,
    email: &str,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let mut tx = db.begin().await?;
    let accepted: Option<(Uuid, String)> = sqlx::query_as(
        "UPDATE workspace_invitations SET accepted_at = now(), accepted_by = $2 \
         WHERE token_hash = $1 AND lower(email) = lower($3) AND accepted_at IS NULL \
           AND revoked_at IS NULL AND expires_at > now() \
         RETURNING workspace_id, role",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((workspace_id, role)) = accepted else {
        return Ok(None);
    };
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3) \
         ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = CASE \
             WHEN array_position(ARRAY['viewer','editor','admin','owner'], EXCLUDED.role) \
                > array_position(ARRAY['viewer','editor','admin','owner'], workspace_members.role) \
             THEN EXCLUDED.role ELSE workspace_members.role END",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(&role)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some((workspace_id, role)))
}

// ---------------------------------------------------------------------------
// Connection grants
// ---------------------------------------------------------------------------

/// Members of the connection's workspace with their explicit grants.
pub async fn grants(db: &PgPool, connection_id: Uuid) -> Result<Vec<GrantRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.user_id, u.email, u.display_name, m.role, g.access AS \"grant\" \
         FROM database_connections c \
         JOIN workspace_members m ON m.workspace_id = c.workspace_id \
         JOIN users u ON u.id = m.user_id \
         LEFT JOIN connection_grants g ON g.connection_id = c.id AND g.user_id = m.user_id \
         WHERE c.id = $1 ORDER BY lower(u.email)",
    )
    .bind(connection_id)
    .fetch_all(db)
    .await
}

pub async fn set_grant(
    db: &PgPool,
    connection_id: Uuid,
    user_id: Uuid,
    access: Option<&str>,
) -> Result<(), sqlx::Error> {
    match access {
        Some(a) => {
            sqlx::query(
                "INSERT INTO connection_grants (connection_id, user_id, access) VALUES ($1, $2, $3) \
                 ON CONFLICT (connection_id, user_id) DO UPDATE SET access = EXCLUDED.access, updated_at = now()",
            )
            .bind(connection_id)
            .bind(user_id)
            .bind(a)
            .execute(db)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM connection_grants WHERE connection_id = $1 AND user_id = $2")
                .bind(connection_id)
                .bind(user_id)
                .execute(db)
                .await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Snippets
// ---------------------------------------------------------------------------

const SNIPPET_SELECT: &str =
    "SELECT s.id, s.workspace_id, s.created_by, u.email AS created_by_email, s.name, \
            s.description, s.body, s.tags, s.created_at, s.updated_at \
     FROM snippets s LEFT JOIN users u ON u.id = s.created_by";

pub async fn list_snippets(db: &PgPool, workspace_id: Uuid) -> Result<Vec<Snippet>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{SNIPPET_SELECT} WHERE s.workspace_id = $1 ORDER BY lower(s.name)"
    ))
    .bind(workspace_id)
    .fetch_all(db)
    .await
}

pub async fn get_snippet(db: &PgPool, id: Uuid) -> Result<Option<Snippet>, sqlx::Error> {
    sqlx::query_as(&format!("{SNIPPET_SELECT} WHERE s.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_snippet(
    db: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    name: &str,
    description: Option<&str>,
    body: &str,
    tags: &[String],
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO snippets (id, workspace_id, created_by, name, description, body, tags) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(body)
    .bind(tags)
    .execute(db)
    .await?;
    Ok(id)
}

pub async fn update_snippet(
    db: &PgPool,
    id: Uuid,
    name: &str,
    description: Option<&str>,
    body: &str,
    tags: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE snippets SET name = $2, description = $3, body = $4, tags = $5 WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(body)
    .bind(tags)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete_snippet(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM snippets WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

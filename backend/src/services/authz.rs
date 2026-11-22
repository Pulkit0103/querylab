//! Authorization: who may do what in a workspace and on a connection.
//!
//! Every connection belongs to a workspace. A user can see it only as a
//! member of that workspace with an effective access other than `none`;
//! everything else answers 404, so ids cannot be probed. Effective access is
//! the member's grant on the connection, or their role's default, capped
//! by the role (viewers never write); owners and admins manage everything.
//!
//! Credentials are never returned by any endpoint, so members use shared
//! connections without seeing them.

use uuid::Uuid;

use crate::{
    database::{connections as connections_repo, workspaces as repo},
    errors::{AppError, AppResult},
    models::{
        connection::ConnectionRecord,
        workspaces::{Access, Role},
    },
    state::AppState,
};

/// A member's view of one connection.
#[derive(Debug, Clone)]
pub struct ConnectionAccess {
    pub record: ConnectionRecord,
    pub role: Role,
    pub access: Access,
}

impl ConnectionAccess {
    /// Writes need write access and a read-write connection.
    pub fn can_write(&self) -> bool {
        self.access >= Access::Write && !self.record.read_only
    }
}

/// The user's role in a workspace; 404 when not a member.
pub async fn workspace_role(
    state: &AppState,
    user_id: Uuid,
    workspace_id: Uuid,
) -> AppResult<(Role, bool)> {
    let (role, personal) = repo::role(&state.db, user_id, workspace_id)
        .await?
        .ok_or(AppError::NotFound("workspace"))?;
    let role =
        Role::parse(&role).ok_or_else(|| AppError::internal(format!("unknown role {role}")))?;
    Ok((role, personal))
}

/// Requires at least `min` in the workspace.
pub async fn require_role(
    state: &AppState,
    user_id: Uuid,
    workspace_id: Uuid,
    min: Role,
) -> AppResult<(Role, bool)> {
    let (role, personal) = workspace_role(state, user_id, workspace_id).await?;
    if role < min {
        return Err(AppError::Forbidden(format!(
            "This needs the {} role or higher in the workspace.",
            min.as_str()
        )));
    }
    Ok((role, personal))
}

/// The workspace a request targets: the given one (membership checked) or
/// the user's personal workspace.
pub async fn resolve_workspace(
    state: &AppState,
    user_id: Uuid,
    workspace_id: Option<Uuid>,
) -> AppResult<Uuid> {
    match workspace_id {
        Some(id) => {
            workspace_role(state, user_id, id).await?;
            Ok(id)
        }
        None => Ok(repo::ensure_personal(&state.db, user_id).await?),
    }
}

/// Loads a connection the user may use with at least `need` access.
pub async fn connection(
    state: &AppState,
    user_id: Uuid,
    connection_id: Uuid,
    need: Access,
) -> AppResult<ConnectionAccess> {
    let (record, role, grant) =
        connections_repo::find_with_access(&state.db, user_id, connection_id)
            .await?
            .ok_or(AppError::NotFound("connection"))?;
    let role =
        Role::parse(&role).ok_or_else(|| AppError::internal(format!("unknown role {role}")))?;
    let access = Access::effective(role, grant.as_deref().and_then(Access::parse));
    if access == Access::None {
        return Err(AppError::NotFound("connection"));
    }
    if access < need {
        return Err(AppError::Forbidden(match need {
            Access::Admin => {
                "Only administrators of this connection can change or share it.".into()
            }
            _ => "You have read-only access to this connection.".into(),
        }));
    }
    Ok(ConnectionAccess {
        record,
        role,
        access,
    })
}

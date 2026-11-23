//! Team workspaces: membership, roles, invitations and snippets.
//!
//! Rules:
//! * Any member can read the workspace, its members and its snippets.
//! * Admins and owners manage members and invitations; only owners can
//!   make or remove owners, and a workspace always keeps one owner.
//! * Editors and above write snippets (and saved queries, folders and
//!   connections); viewers only read.
//! * Personal workspaces have exactly one member and cannot be shared or
//!   deleted.

use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::{
    database::workspaces as repo,
    errors::{AppError, AppResult},
    models::workspaces::{
        CreatedInvitation, Invitation, InvitationPreview, InviteRequest, Member, Role, Snippet,
        SnippetRequest, WorkspaceSummary,
    },
    services::{
        audit::{self, AuditEvent},
        auth::{generate_token, hash_token, AuthUser},
        authz,
        mailer::Email,
    },
    state::AppState,
};

const INVITATION_DAYS: i64 = 7;

pub struct WorkspaceService<'a> {
    state: &'a AppState,
}

impl<'a> WorkspaceService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    async fn event(
        &self,
        user: Uuid,
        workspace: Uuid,
        action: &'static str,
        detail: serde_json::Value,
    ) {
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user), action)
                .workspace(Some(workspace))
                .target("workspace", workspace)
                .detail(detail),
        )
        .await;
    }

    pub async fn list(&self, user_id: Uuid) -> AppResult<Vec<WorkspaceSummary>> {
        repo::ensure_personal(&self.state.db, user_id).await?;
        Ok(repo::list_for_user(&self.state.db, user_id).await?)
    }

    pub async fn get(&self, user_id: Uuid, id: Uuid) -> AppResult<WorkspaceSummary> {
        repo::get_for_user(&self.state.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("workspace"))
    }

    pub async fn create(&self, user_id: Uuid, name: &str) -> AppResult<WorkspaceSummary> {
        let id = repo::create(&self.state.db, user_id, name.trim()).await?;
        self.event(
            user_id,
            id,
            "workspace.created",
            json!({ "name": name.trim() }),
        )
        .await;
        self.get(user_id, id).await
    }

    pub async fn rename(&self, user_id: Uuid, id: Uuid, name: &str) -> AppResult<WorkspaceSummary> {
        authz::require_role(self.state, user_id, id, Role::Admin).await?;
        repo::rename(&self.state.db, id, name.trim()).await?;
        self.event(
            user_id,
            id,
            "workspace.renamed",
            json!({ "name": name.trim() }),
        )
        .await;
        self.get(user_id, id).await
    }

    /// Deletes a team workspace with its connections, folders, saved
    /// queries and snippets (owners only).
    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        let (_, personal) = authz::require_role(self.state, user_id, id, Role::Owner).await?;
        if personal {
            return Err(AppError::BadRequest(
                "a personal workspace cannot be deleted".into(),
            ));
        }
        self.event(user_id, id, "workspace.deleted", json!({}))
            .await;
        repo::delete(&self.state.db, id).await?;
        Ok(())
    }

    // -- members ------------------------------------------------------------

    pub async fn members(&self, user_id: Uuid, id: Uuid) -> AppResult<Vec<Member>> {
        authz::workspace_role(self.state, user_id, id).await?;
        Ok(repo::members(&self.state.db, id).await?)
    }

    async fn member_role(&self, workspace_id: Uuid, member: Uuid) -> AppResult<Role> {
        authz::workspace_role(self.state, member, workspace_id)
            .await
            .map(|(r, _)| r)
            .map_err(|_| AppError::NotFound("member"))
    }

    pub async fn set_role(
        &self,
        user_id: Uuid,
        id: Uuid,
        member: Uuid,
        role: Role,
    ) -> AppResult<Vec<Member>> {
        let (mine, personal) = authz::require_role(self.state, user_id, id, Role::Admin).await?;
        if personal {
            return Err(AppError::BadRequest(
                "a personal workspace has a single owner".into(),
            ));
        }
        let current = self.member_role(id, member).await?;
        // Only owners create or demote owners.
        if (role == Role::Owner || current == Role::Owner) && mine != Role::Owner {
            return Err(AppError::Forbidden(
                "Only owners can change who owns the workspace.".into(),
            ));
        }
        if current == Role::Owner
            && role != Role::Owner
            && repo::owner_count(&self.state.db, id).await? <= 1
        {
            return Err(AppError::Conflict(
                "a workspace must keep at least one owner".into(),
            ));
        }
        repo::set_role(&self.state.db, id, member, role.as_str()).await?;
        self.event(
            user_id,
            id,
            "permission.role_changed",
            json!({ "member": member, "from": current.as_str(), "to": role.as_str() }),
        )
        .await;
        self.members(user_id, id).await
    }

    /// Removes a member (admins and owners), or leaves (anyone).
    pub async fn remove_member(&self, user_id: Uuid, id: Uuid, member: Uuid) -> AppResult<()> {
        let (mine, personal) = authz::workspace_role(self.state, user_id, id).await?;
        if personal {
            return Err(AppError::BadRequest(
                "you cannot leave your personal workspace".into(),
            ));
        }
        let leaving = member == user_id;
        if !leaving && mine < Role::Admin {
            return Err(AppError::Forbidden(
                "Only admins and owners can remove members.".into(),
            ));
        }
        let current = self.member_role(id, member).await?;
        if current == Role::Owner {
            if !leaving && mine != Role::Owner {
                return Err(AppError::Forbidden(
                    "Only owners can remove an owner.".into(),
                ));
            }
            if repo::owner_count(&self.state.db, id).await? <= 1 {
                return Err(AppError::Conflict(
                    "a workspace must keep at least one owner; transfer ownership or delete the workspace".into(),
                ));
            }
        }
        repo::remove_member(&self.state.db, id, member).await?;
        self.event(
            user_id,
            id,
            if leaving {
                "permission.member_left"
            } else {
                "permission.member_removed"
            },
            json!({ "member": member, "role": current.as_str() }),
        )
        .await;
        Ok(())
    }

    // -- invitations ----------------------------------------------------------

    pub async fn invite(
        &self,
        user: &AuthUser,
        id: Uuid,
        req: &InviteRequest,
    ) -> AppResult<CreatedInvitation> {
        let (mine, personal) =
            authz::require_role(self.state, user.user_id, id, Role::Admin).await?;
        if personal {
            return Err(AppError::BadRequest(
                "a personal workspace cannot be shared; create a team workspace".into(),
            ));
        }
        if req.role == Role::Admin && mine != Role::Owner && mine != Role::Admin {
            return Err(AppError::Forbidden(
                "Only admins and owners can invite admins.".into(),
            ));
        }
        let email = req.email.trim();
        let token = generate_token();
        let invitation = repo::insert_invitation(
            &self.state.db,
            id,
            email,
            req.role.as_str(),
            &hash_token(&token),
            user.user_id,
            Utc::now() + ChronoDuration::days(INVITATION_DAYS),
        )
        .await?;
        let link = format!(
            "{}/invite?token={}",
            self.state.config.app_base_url,
            token.as_str()
        );
        let workspace = self.get(user.user_id, id).await?;
        let mut emailed = false;
        if self.state.mailer.is_enabled() {
            let text = format!(
                "{} invited you to the \"{}\" workspace on QueryLab as {}.\n\n\
                 Open this link within {INVITATION_DAYS} days to join (sign in or create an account with this email address):\n{link}",
                user.email,
                workspace.name,
                req.role.as_str()
            );
            match self
                .state
                .mailer
                .send(Email {
                    to: email,
                    subject: "You are invited to a QueryLab workspace",
                    text,
                })
                .await
            {
                Ok(()) => emailed = true,
                Err(err) => tracing::warn!(error = %err, "invitation email failed"),
            }
        }
        self.event(
            user.user_id,
            id,
            "permission.invitation_created",
            json!({ "email": email, "role": req.role.as_str() }),
        )
        .await;
        Ok(CreatedInvitation {
            invitation,
            link,
            emailed,
        })
    }

    pub async fn invitations(&self, user_id: Uuid, id: Uuid) -> AppResult<Vec<Invitation>> {
        authz::require_role(self.state, user_id, id, Role::Admin).await?;
        Ok(repo::pending_invitations(&self.state.db, id).await?)
    }

    pub async fn revoke_invitation(
        &self,
        user_id: Uuid,
        id: Uuid,
        invitation: Uuid,
    ) -> AppResult<()> {
        authz::require_role(self.state, user_id, id, Role::Admin).await?;
        if !repo::revoke_invitation(&self.state.db, id, invitation).await? {
            return Err(AppError::NotFound("invitation"));
        }
        self.event(
            user_id,
            id,
            "permission.invitation_revoked",
            json!({ "invitation": invitation }),
        )
        .await;
        Ok(())
    }

    pub async fn preview_invitation(&self, token: &str) -> AppResult<InvitationPreview> {
        repo::preview_invitation(&self.state.db, &hash_token(token))
            .await?
            .ok_or(AppError::NotFound("invitation"))
    }

    /// Joins the workspace. The invitation is bound to an email address: the
    /// signed-in account must use it.
    pub async fn accept(&self, user: &AuthUser, token: &str) -> AppResult<WorkspaceSummary> {
        let hash = hash_token(token);
        let preview = repo::preview_invitation(&self.state.db, &hash)
            .await?
            .ok_or(AppError::NotFound("invitation"))?;
        if !preview.email.eq_ignore_ascii_case(&user.email) {
            return Err(AppError::Forbidden(format!(
                "This invitation is for {}. Sign in with that account to accept it.",
                preview.email
            )));
        }
        let (workspace_id, role) =
            repo::accept_invitation(&self.state.db, &hash, user.user_id, &user.email)
                .await?
                .ok_or(AppError::NotFound("invitation"))?;
        self.event(
            user.user_id,
            workspace_id,
            "permission.invitation_accepted",
            json!({ "role": role }),
        )
        .await;
        self.get(user.user_id, workspace_id).await
    }

    // -- snippets -------------------------------------------------------------

    pub async fn snippets(
        &self,
        user_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> AppResult<Vec<Snippet>> {
        let workspace_id = authz::resolve_workspace(self.state, user_id, workspace_id).await?;
        Ok(repo::list_snippets(&self.state.db, workspace_id).await?)
    }

    async fn snippet_for(&self, user_id: Uuid, id: Uuid, min: Role) -> AppResult<Snippet> {
        let snippet = repo::get_snippet(&self.state.db, id)
            .await?
            .ok_or(AppError::NotFound("snippet"))?;
        authz::workspace_role(self.state, user_id, snippet.workspace_id)
            .await
            .map_err(|_| AppError::NotFound("snippet"))?;
        authz::require_role(self.state, user_id, snippet.workspace_id, min).await?;
        Ok(snippet)
    }

    pub async fn snippet(&self, user_id: Uuid, id: Uuid) -> AppResult<Snippet> {
        self.snippet_for(user_id, id, Role::Viewer).await
    }

    pub async fn create_snippet(&self, user_id: Uuid, req: &SnippetRequest) -> AppResult<Snippet> {
        let workspace_id = authz::resolve_workspace(self.state, user_id, req.workspace_id).await?;
        authz::require_role(self.state, user_id, workspace_id, Role::Editor).await?;
        let tags = crate::models::library::normalize_tags(&req.tags);
        let id = repo::insert_snippet(
            &self.state.db,
            workspace_id,
            user_id,
            req.name.trim(),
            req.description
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty()),
            &req.body,
            &tags,
        )
        .await
        .map_err(name_conflict)?;
        self.snippet(user_id, id).await
    }

    pub async fn update_snippet(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &SnippetRequest,
    ) -> AppResult<Snippet> {
        self.snippet_for(user_id, id, Role::Editor).await?;
        let tags = crate::models::library::normalize_tags(&req.tags);
        repo::update_snippet(
            &self.state.db,
            id,
            req.name.trim(),
            req.description
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty()),
            &req.body,
            &tags,
        )
        .await
        .map_err(name_conflict)?;
        self.snippet(user_id, id).await
    }

    pub async fn delete_snippet(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        self.snippet_for(user_id, id, Role::Editor).await?;
        repo::delete_snippet(&self.state.db, id).await?;
        Ok(())
    }
}

fn name_conflict(err: sqlx::Error) -> AppError {
    match &err {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            AppError::Conflict("a snippet with this name already exists".into())
        }
        _ => AppError::from(err),
    }
}

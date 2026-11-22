//! Team workspaces, roles, connection grants, invitations and snippets.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::auth::is_plausible_email,
    utils::validation::{Validate, Validator},
};

/// A member's role in a workspace, from least to most privileged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Editor,
    Admin,
    Owner,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Editor => "editor",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "viewer" => Role::Viewer,
            "editor" => Role::Editor,
            "admin" => Role::Admin,
            "owner" => Role::Owner,
            _ => return None,
        })
    }

    /// Connection access a member has without an explicit grant.
    pub fn default_access(self) -> Access {
        match self {
            Role::Viewer => Access::Read,
            Role::Editor => Access::Write,
            Role::Admin | Role::Owner => Access::Admin,
        }
    }

    /// The most a grant can give this role: viewers never write.
    pub fn max_access(self) -> Access {
        match self {
            Role::Viewer => Access::Read,
            _ => Access::Admin,
        }
    }
}

/// Access to one connection, from none to managing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Not visible at all.
    None,
    /// Browse schemas and data, run read-only statements, export.
    Read,
    /// Also run writes (on read-write connections), apply changes, import,
    /// generate data, run migrations, cancel sessions.
    Write,
    /// Also edit, delete and share the connection.
    Admin,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::None => "none",
            Access::Read => "read",
            Access::Write => "write",
            Access::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Access::None,
            "read" => Access::Read,
            "write" => Access::Write,
            "admin" => Access::Admin,
            _ => return None,
        })
    }

    /// Effective access of a member: workspace owners and admins manage
    /// every connection; others get their grant (or their role's default),
    /// capped by what their role allows.
    pub fn effective(role: Role, grant: Option<Access>) -> Access {
        if role >= Role::Admin {
            return Access::Admin;
        }
        grant
            .unwrap_or(role.default_access())
            .min(role.max_access())
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub name: String,
    pub personal: bool,
    pub role: String,
    pub member_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Member {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: String,
    pub invited_by_email: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// What an invitee sees before accepting.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InvitationPreview {
    pub workspace_name: String,
    pub email: String,
    pub role: String,
    pub invited_by_email: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CreatedInvitation {
    pub invitation: Invitation,
    /// Link to share with the invitee (also emailed when email is set up).
    pub link: String,
    pub emailed: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GrantRow {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    /// Explicit grant, if any.
    pub grant: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemberAccess {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub role: Role,
    pub grant: Option<Access>,
    pub effective: Access,
}

fn validate_name(v: &mut Validator, name: &str) {
    v.length("name", name.trim(), 1, 100)
        .no_control_chars("name", name);
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceRequest {
    pub name: String,
}

impl Validate for WorkspaceRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_name(&mut v, &self.name);
        v.finish()
    }
}

#[derive(Debug, Deserialize)]
pub struct RoleRequest {
    pub role: Role,
}

impl Validate for RoleRequest {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct InviteRequest {
    pub email: String,
    pub role: Role,
}

impl Validate for InviteRequest {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .length("email", self.email.trim(), 3, 254)
            .check(
                is_plausible_email(self.email.trim()),
                "email",
                "must be a valid email address",
            )
            .check(
                self.role != Role::Owner,
                "role",
                "invite as admin, editor or viewer",
            )
            .finish()
    }
}

#[derive(Debug, Deserialize)]
pub struct GrantRequest {
    /// `null` removes the explicit grant (back to the role's default).
    pub access: Option<Access>,
}

impl Validate for GrantRequest {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

/// `?workspace_id=` on list endpoints; defaults to the personal workspace.
#[derive(Debug, Default, Deserialize)]
pub struct WorkspaceScope {
    pub workspace_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Snippets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Snippet {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub created_by: Option<Uuid>,
    pub created_by_email: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SnippetRequest {
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Validate for SnippetRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_name(&mut v, &self.name);
        if let Some(d) = &self.description {
            v.length("description", d, 0, 500);
        }
        v.check(
            !self.body.trim().is_empty() && self.body.len() <= 100 * 1024,
            "body",
            "must be 1 byte to 100 KB",
        );
        v.check(self.tags.len() <= 20, "tags", "at most 20 tags");
        for t in &self.tags {
            v.length("tags", t.trim(), 1, 50);
        }
        v.finish()
    }
}

/// `{{name}}` placeholders of a snippet body, in order of appearance.
pub fn placeholders(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        let valid = !name.is_empty()
            && name.len() <= 50
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '));
        if valid && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_access_follows_roles_and_grants() {
        let e = Access::effective;
        assert_eq!(e(Role::Owner, Some(Access::None)), Access::Admin);
        assert_eq!(e(Role::Admin, Some(Access::Read)), Access::Admin);
        assert_eq!(e(Role::Editor, Option::None), Access::Write);
        assert_eq!(e(Role::Editor, Some(Access::Read)), Access::Read);
        assert_eq!(e(Role::Editor, Some(Access::None)), Access::None);
        assert_eq!(e(Role::Editor, Some(Access::Admin)), Access::Admin);
        assert_eq!(e(Role::Viewer, Option::None), Access::Read);
        // Viewers never write, whatever the grant says.
        assert_eq!(e(Role::Viewer, Some(Access::Write)), Access::Read);
        assert_eq!(e(Role::Viewer, Some(Access::None)), Access::None);
        assert!(Role::Owner > Role::Admin && Role::Editor > Role::Viewer);
    }

    #[test]
    fn placeholders_are_unique_and_ordered() {
        assert_eq!(
            placeholders(
                "select * from {{table}} where id = {{ id }} and t = '{{table}}' {{}} {{bad!}}"
            ),
            ["table", "id"]
        );
        assert!(placeholders("no placeholders {{ unterminated").is_empty());
    }

    #[test]
    fn invitations_cannot_make_owners() {
        let req = InviteRequest {
            email: "a@b.co".into(),
            role: Role::Owner,
        };
        assert!(req.validate().is_err());
    }
}

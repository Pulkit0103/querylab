//! Business logic for database connections, their groups and who may use
//! them. Connections belong to workspaces; access is decided by
//! [`authz`](crate::services::authz).

use serde_json::json;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    database::{
        connections::{self as repo, NewConnection},
        workspaces as workspaces_repo,
    },
    errors::{AppError, AppResult},
    models::{
        connection::{
            ConnectionGroup, ConnectionGroupRequest, ConnectionRecord, ConnectionResponse,
            ConnectionSecret, ConnectionStatus, CreateConnectionRequest, UpdateConnectionRequest,
        },
        workspaces::{Access, MemberAccess, Role},
    },
    services::{
        audit::{self, AuditEvent},
        authz::{self, ConnectionAccess},
        target_db::{self, ConnectionTestResult, TargetSpec},
    },
    state::AppState,
    utils::network,
};

/// Stored error messages are truncated to this many characters.
const MAX_STATUS_ERROR_CHARS: usize = 500;

pub struct ConnectionService<'a> {
    state: &'a AppState,
}

/// Associated data binding a ciphertext to its creator and row.
fn credentials_aad(user_id: Uuid, connection_id: Uuid) -> Vec<u8> {
    format!("querylab/connection/v1/{user_id}/{connection_id}").into_bytes()
}

fn normalise_color(color: Option<&str>) -> Option<String> {
    color.map(|c| c.to_ascii_lowercase())
}

fn normalise_pem(pem: Option<&str>) -> Option<String> {
    pem.map(|p| format!("{}\n", p.trim()))
}

impl<'a> ConnectionService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Connections the user can see, optionally in one workspace.
    pub async fn list(
        &self,
        user_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> AppResult<Vec<ConnectionResponse>> {
        let rows = repo::list_with_access(&self.state.db, user_id, workspace_id).await?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let role = Role::parse(&r.member_role)?;
                let access =
                    Access::effective(role, r.grant_access.as_deref().and_then(Access::parse));
                (access != Access::None).then(|| ConnectionResponse::with_access(&r.record, access))
            })
            .collect())
    }

    /// Loads a connection the user may read. Returns 404 (not 403) for
    /// connections the user cannot see, so ids cannot be probed.
    pub async fn get_record(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionRecord> {
        Ok(authz::connection(self.state, user_id, id, Access::Read)
            .await?
            .record)
    }

    /// Loads a connection the user may use with at least `need` access.
    pub async fn require(
        &self,
        user_id: Uuid,
        id: Uuid,
        need: Access,
    ) -> AppResult<ConnectionAccess> {
        authz::connection(self.state, user_id, id, need).await
    }

    pub async fn get(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionResponse> {
        let a = self.require(user_id, id, Access::Read).await?;
        Ok(ConnectionResponse::with_access(&a.record, a.access))
    }

    fn encrypt_secret(&self, creator: Uuid, id: Uuid, password: &str) -> AppResult<Vec<u8>> {
        let secret = ConnectionSecret {
            password: password.to_string(),
        };
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&secret)
                .map_err(|e| AppError::internal(format!("serialise secret: {e}")))?,
        );
        self.state
            .cipher
            .encrypt(&plaintext, &credentials_aad(creator, id))
            .map_err(|e| AppError::internal(format!("encrypt credentials: {e}")))
    }

    /// Decrypts the stored secret of a connection the caller may use.
    pub fn decrypt_secret(&self, record: &ConnectionRecord) -> AppResult<ConnectionSecret> {
        let plaintext = self
            .state
            .cipher
            .decrypt(
                &record.encrypted_credentials,
                &credentials_aad(record.user_id, record.id),
            )
            .map_err(|e| {
                AppError::internal(format!("decrypt credentials for {}: {e}", record.id))
            })?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| AppError::internal(format!("parse credentials for {}: {e}", record.id)))
    }

    /// Creates a connection in a workspace (editors and above).
    pub async fn create(
        &self,
        user_id: Uuid,
        req: &CreateConnectionRequest,
    ) -> AppResult<ConnectionResponse> {
        let workspace_id = authz::resolve_workspace(self.state, user_id, req.workspace_id).await?;
        let (role, _) =
            authz::require_role(self.state, user_id, workspace_id, Role::Editor).await?;
        network::precheck(
            req.host.trim(),
            req.port,
            self.state.config.allow_private_networks,
        )?;
        let id = Uuid::new_v4();
        let encrypted = self.encrypt_secret(user_id, id, &req.password)?;
        let color = normalise_color(req.color.as_deref());
        let pem = normalise_pem(req.ssl_root_cert.as_deref());
        let record = repo::insert(
            &self.state.db,
            &NewConnection {
                id,
                user_id,
                workspace_id,
                name: req.name.trim(),
                host: req.host.trim(),
                port: req.port,
                database_name: &req.database,
                username: &req.username,
                ssl_mode: req.ssl_mode.as_str(),
                read_only: req.read_only,
                encrypted_credentials: &encrypted,
                environment: req.environment.as_str(),
                group_id: req.group_id,
                color: color.as_deref(),
                ssl_root_cert: pem.as_deref(),
            },
        )
        .await
        .map_err(write_error)?;
        self.grant_creator(role, record.id, user_id).await?;
        tracing::info!(connection_id = %record.id, %user_id, "connection created");
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user_id), "connection.created")
                .connection(record.id)
                .workspace(Some(workspace_id))
                .detail(json!({ "name": record.name, "environment": record.environment, "read_only": record.read_only })),
        )
        .await;
        Ok(ConnectionResponse::with_access(&record, Access::Admin))
    }

    pub async fn update(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &UpdateConnectionRequest,
    ) -> AppResult<ConnectionResponse> {
        let existing = self.require(user_id, id, Access::Admin).await?.record;
        // The ciphertext stays bound to the creator whoever edits it.
        let creator = existing.user_id;
        let encrypted = match &req.password {
            Some(password) => self.encrypt_secret(creator, id, password)?,
            None if self
                .state
                .cipher
                .needs_rotation(&existing.encrypted_credentials) =>
            {
                // Opportunistically re-encrypt with the current key.
                let secret = self.decrypt_secret(&existing)?;
                self.encrypt_secret(creator, id, &secret.password)?
            }
            None => existing.encrypted_credentials.clone(),
        };
        let name = req.name.as_deref().map(str::trim).unwrap_or(&existing.name);
        let host = req.host.as_deref().map(str::trim).unwrap_or(&existing.host);
        let port = req.port.unwrap_or(existing.port());
        let database_name = req.database.as_deref().unwrap_or(&existing.database_name);
        let username = req.username.as_deref().unwrap_or(&existing.username);
        let ssl_mode = req.ssl_mode.unwrap_or(existing.ssl_mode());
        let color = match &req.color {
            Some(c) => normalise_color(c.as_deref()),
            None => existing.color.clone(),
        };
        let pem = match &req.ssl_root_cert {
            Some(p) => normalise_pem(p.as_deref()),
            None => existing.ssl_root_cert.clone(),
        };
        network::precheck(host, port, self.state.config.allow_private_networks)?;

        // The last test result no longer applies once the target changes.
        let reset_status = host != existing.host
            || port != existing.port()
            || database_name != existing.database_name
            || username != existing.username
            || req.password.is_some()
            || ssl_mode != existing.ssl_mode()
            || pem != existing.ssl_root_cert;

        let record = repo::update(
            &self.state.db,
            &NewConnection {
                id,
                user_id: creator,
                workspace_id: existing.workspace_id,
                name,
                host,
                port,
                database_name,
                username,
                ssl_mode: ssl_mode.as_str(),
                read_only: req.read_only.unwrap_or(existing.read_only),
                encrypted_credentials: &encrypted,
                environment: req.environment.unwrap_or(existing.environment()).as_str(),
                group_id: req.group_id.unwrap_or(existing.group_id),
                color: color.as_deref(),
                ssl_root_cert: pem.as_deref(),
            },
            reset_status,
        )
        .await
        .map_err(write_error)?
        .ok_or(AppError::NotFound("connection"))?;
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user_id), "connection.updated")
                .connection(id)
                .workspace(Some(record.workspace_id))
                .detail(json!({
                    "target_changed": reset_status,
                    "password_changed": req.password.is_some(),
                    "read_only": record.read_only,
                    "environment": record.environment,
                })),
        )
        .await;
        Ok(ConnectionResponse::with_access(&record, Access::Admin))
    }

    /// Copies a connection, including its credentials, under a free
    /// "<name> (copy)" name in the same workspace. The secret is
    /// re-encrypted because the ciphertext is bound to the connection id.
    pub async fn duplicate(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionResponse> {
        let source = self.require(user_id, id, Access::Admin).await?.record;
        let (role, _) =
            authz::require_role(self.state, user_id, source.workspace_id, Role::Editor).await?;
        let secret = self.decrypt_secret(&source)?;
        let name = self
            .free_copy_name(source.workspace_id, &source.name)
            .await?;
        let new_id = Uuid::new_v4();
        let encrypted = self.encrypt_secret(user_id, new_id, &secret.password)?;
        let record = repo::insert(
            &self.state.db,
            &NewConnection {
                id: new_id,
                user_id,
                workspace_id: source.workspace_id,
                name: &name,
                host: &source.host,
                port: source.port(),
                database_name: &source.database_name,
                username: &source.username,
                ssl_mode: &source.ssl_mode,
                read_only: source.read_only,
                encrypted_credentials: &encrypted,
                environment: &source.environment,
                group_id: source.group_id,
                color: source.color.as_deref(),
                ssl_root_cert: source.ssl_root_cert.as_deref(),
            },
        )
        .await
        .map_err(write_error)?;
        self.grant_creator(role, record.id, user_id).await?;
        tracing::info!(connection_id = %record.id, source_id = %id, %user_id, "connection duplicated");
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user_id), "connection.created")
                .connection(record.id)
                .workspace(Some(record.workspace_id))
                .detail(json!({ "name": record.name, "duplicated_from": id })),
        )
        .await;
        Ok(ConnectionResponse::with_access(&record, Access::Admin))
    }

    /// Whoever creates a connection manages it (owners and admins already
    /// manage every connection).
    async fn grant_creator(&self, role: Role, connection_id: Uuid, user_id: Uuid) -> AppResult<()> {
        if role < Role::Admin {
            workspaces_repo::set_grant(
                &self.state.db,
                connection_id,
                user_id,
                Some(Access::Admin.as_str()),
            )
            .await?;
        }
        Ok(())
    }

    async fn free_copy_name(&self, workspace_id: Uuid, base: &str) -> AppResult<String> {
        // Leave room for the suffix within the 100-character limit.
        let stem: String = base.chars().take(85).collect();
        for n in 1..=50 {
            let candidate = if n == 1 {
                format!("{stem} (copy)")
            } else {
                format!("{stem} (copy {n})")
            };
            if !repo::name_exists(&self.state.db, workspace_id, &candidate).await? {
                return Ok(candidate);
            }
        }
        Err(AppError::Conflict(
            "too many copies of this connection; rename some first".into(),
        ))
    }

    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        let a = self.require(user_id, id, Access::Admin).await?;
        if repo::delete(&self.state.db, id).await? {
            tracing::info!(connection_id = %id, %user_id, "connection deleted");
            audit::record(
                &self.state.db,
                AuditEvent::new(Some(user_id), "connection.deleted")
                    .target("connection", id)
                    .workspace(Some(a.record.workspace_id))
                    .detail(json!({ "name": a.record.name })),
            )
            .await;
            Ok(())
        } else {
            Err(AppError::NotFound("connection"))
        }
    }

    /// Tests a saved connection and records the outcome as its status. A
    /// failed test is still recorded before the error is returned.
    pub async fn test_saved(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionTestResult> {
        let record = self.get_record(user_id, id).await?;
        let secret = self.decrypt_secret(&record)?;
        let outcome = target_db::test_connection(
            &self.state.config,
            &TargetSpec::from_record(&record, &secret),
        )
        .await;
        match outcome {
            Ok(result) => {
                let latency = i32::try_from(result.latency_ms).unwrap_or(i32::MAX);
                repo::record_status(
                    &self.state.db,
                    id,
                    ConnectionStatus::Online.as_str(),
                    Some(latency),
                    None,
                )
                .await?;
                Ok(result)
            }
            Err(failure) => {
                let message: String = failure
                    .error
                    .to_string()
                    .chars()
                    .take(MAX_STATUS_ERROR_CHARS)
                    .collect();
                repo::record_status(
                    &self.state.db,
                    id,
                    failure.status.as_str(),
                    None,
                    Some(&message),
                )
                .await?;
                Err(*failure.error)
            }
        }
    }

    // -- access -------------------------------------------------------------

    /// Workspace members with their access to the connection.
    pub async fn grants(&self, user_id: Uuid, id: Uuid) -> AppResult<Vec<MemberAccess>> {
        self.require(user_id, id, Access::Admin).await?;
        let rows = workspaces_repo::grants(&self.state.db, id).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let role = Role::parse(&r.role)?;
                let grant = r.grant.as_deref().and_then(Access::parse);
                Some(MemberAccess {
                    user_id: r.user_id,
                    email: r.email,
                    display_name: r.display_name,
                    role,
                    grant,
                    effective: Access::effective(role, grant),
                })
            })
            .collect())
    }

    /// Sets (or clears) a member's explicit access to the connection.
    pub async fn set_grant(
        &self,
        user_id: Uuid,
        id: Uuid,
        member: Uuid,
        access: Option<Access>,
    ) -> AppResult<Vec<MemberAccess>> {
        let a = self.require(user_id, id, Access::Admin).await?;
        let (role, _) = authz::workspace_role(self.state, member, a.record.workspace_id)
            .await
            .map_err(|_| AppError::NotFound("member"))?;
        if role >= Role::Admin {
            return Err(AppError::validation(
                "access",
                "workspace owners and admins always manage every connection",
            ));
        }
        if role == Role::Viewer && access.is_some_and(|x| x > Access::Read) {
            return Err(AppError::validation(
                "access",
                "viewers can have at most read access; change their role first",
            ));
        }
        workspaces_repo::set_grant(&self.state.db, id, member, access.map(Access::as_str)).await?;
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user_id), "permission.grant_changed")
                .connection(id)
                .workspace(Some(a.record.workspace_id))
                .target("user", member)
                .detail(json!({ "access": access.map(Access::as_str) })),
        )
        .await;
        self.grants(user_id, id).await
    }

    // -- groups -------------------------------------------------------------

    pub async fn groups(
        &self,
        user_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> AppResult<Vec<ConnectionGroup>> {
        let workspace_id = authz::resolve_workspace(self.state, user_id, workspace_id).await?;
        Ok(repo::list_groups(&self.state.db, workspace_id).await?)
    }

    pub async fn create_group(
        &self,
        user_id: Uuid,
        req: &ConnectionGroupRequest,
    ) -> AppResult<ConnectionGroup> {
        let workspace_id = authz::resolve_workspace(self.state, user_id, req.workspace_id).await?;
        authz::require_role(self.state, user_id, workspace_id, Role::Editor).await?;
        let position = match req.position {
            Some(p) => p,
            None => repo::next_group_position(&self.state.db, workspace_id).await?,
        };
        repo::insert_group(
            &self.state.db,
            user_id,
            workspace_id,
            req.name.trim(),
            position,
        )
        .await
        .map_err(group_write_error)
    }

    async fn group_for_editor(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        let workspace_id = repo::group_workspace(&self.state.db, id)
            .await?
            .ok_or(AppError::NotFound("connection group"))?;
        authz::workspace_role(self.state, user_id, workspace_id)
            .await
            .map_err(|_| AppError::NotFound("connection group"))?;
        authz::require_role(self.state, user_id, workspace_id, Role::Editor).await?;
        Ok(())
    }

    pub async fn update_group(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &ConnectionGroupRequest,
    ) -> AppResult<ConnectionGroup> {
        self.group_for_editor(user_id, id).await?;
        repo::update_group(&self.state.db, id, req.name.trim(), req.position)
            .await
            .map_err(group_write_error)?
            .ok_or(AppError::NotFound("connection group"))
    }

    /// Deletes a group; its connections become ungrouped.
    pub async fn delete_group(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        self.group_for_editor(user_id, id).await?;
        if repo::delete_group(&self.state.db, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("connection group"))
        }
    }
}

/// Unique-name conflicts get a specific message; a foreign key violation
/// means the group is not in the connection's workspace (404).
fn write_error(err: sqlx::Error) -> AppError {
    match sqlstate(&err).as_deref() {
        Some("23505") => AppError::Conflict("a connection with this name already exists".into()),
        Some("23503") => AppError::NotFound("connection group"),
        _ => AppError::from(err),
    }
}

fn group_write_error(err: sqlx::Error) -> AppError {
    match sqlstate(&err).as_deref() {
        Some("23505") => AppError::Conflict("a group with this name already exists".into()),
        _ => AppError::from(err),
    }
}

fn sqlstate(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db) => db.code().map(|c| c.into_owned()),
        _ => None,
    }
}

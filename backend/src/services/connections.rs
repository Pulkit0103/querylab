//! Business logic for user-owned database connections and their groups.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    config::Config,
    database::connections::{self as repo, NewConnection},
    errors::{AppError, AppResult},
    models::connection::{
        ConnectionGroup, ConnectionGroupRequest, ConnectionRecord, ConnectionResponse,
        ConnectionSecret, ConnectionStatus, CreateConnectionRequest, UpdateConnectionRequest,
    },
    services::{
        audit::{self, AuditEvent},
        target_db::{self, ConnectionTestResult, TargetSpec},
    },
    state::AppState,
    utils::{crypto::CredentialCipher, network},
};

/// Stored error messages are truncated to this many characters.
const MAX_STATUS_ERROR_CHARS: usize = 500;

pub struct ConnectionService<'a> {
    db: &'a PgPool,
    cipher: &'a CredentialCipher,
    config: &'a Config,
}

/// Associated data binding a ciphertext to its owner and row.
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
        Self {
            db: &state.db,
            cipher: &state.cipher,
            config: &state.config,
        }
    }

    pub async fn list(&self, user_id: Uuid) -> AppResult<Vec<ConnectionResponse>> {
        let rows = repo::list_for_user(self.db, user_id).await?;
        Ok(rows.iter().map(ConnectionResponse::from).collect())
    }

    /// Loads a connection owned by `user_id`. Returns 404 (not 403) for other
    /// users' connections so ids cannot be probed.
    pub async fn get_record(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionRecord> {
        repo::find_for_user(self.db, user_id, id)
            .await?
            .ok_or(AppError::NotFound("connection"))
    }

    pub async fn get(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionResponse> {
        Ok(ConnectionResponse::from(
            &self.get_record(user_id, id).await?,
        ))
    }

    fn encrypt_secret(&self, user_id: Uuid, id: Uuid, password: &str) -> AppResult<Vec<u8>> {
        let secret = ConnectionSecret {
            password: password.to_string(),
        };
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&secret)
                .map_err(|e| AppError::internal(format!("serialise secret: {e}")))?,
        );
        self.cipher
            .encrypt(&plaintext, &credentials_aad(user_id, id))
            .map_err(|e| AppError::internal(format!("encrypt credentials: {e}")))
    }

    /// Decrypts the stored secret for a connection the caller already owns.
    pub fn decrypt_secret(&self, record: &ConnectionRecord) -> AppResult<ConnectionSecret> {
        let plaintext = self
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

    pub async fn create(
        &self,
        user_id: Uuid,
        req: &CreateConnectionRequest,
    ) -> AppResult<ConnectionResponse> {
        network::precheck(
            req.host.trim(),
            req.port,
            self.config.allow_private_networks,
        )?;
        let id = Uuid::new_v4();
        let encrypted = self.encrypt_secret(user_id, id, &req.password)?;
        let color = normalise_color(req.color.as_deref());
        let pem = normalise_pem(req.ssl_root_cert.as_deref());
        let record = repo::insert(
            self.db,
            &NewConnection {
                id,
                user_id,
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
        tracing::info!(connection_id = %record.id, %user_id, "connection created");
        audit::record(
            self.db,
            AuditEvent::new(Some(user_id), "connection.created")
                .connection(record.id)
                .detail(json!({ "name": record.name, "environment": record.environment, "read_only": record.read_only })),
        )
        .await;
        Ok(ConnectionResponse::from(&record))
    }

    pub async fn update(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &UpdateConnectionRequest,
    ) -> AppResult<ConnectionResponse> {
        let existing = self.get_record(user_id, id).await?;
        let encrypted = match &req.password {
            Some(password) => self.encrypt_secret(user_id, id, password)?,
            None if self.cipher.needs_rotation(&existing.encrypted_credentials) => {
                // Opportunistically re-encrypt with the current key.
                let secret = self.decrypt_secret(&existing)?;
                self.encrypt_secret(user_id, id, &secret.password)?
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
        network::precheck(host, port, self.config.allow_private_networks)?;

        // The last test result no longer applies once the target changes.
        let reset_status = host != existing.host
            || port != existing.port()
            || database_name != existing.database_name
            || username != existing.username
            || req.password.is_some()
            || ssl_mode != existing.ssl_mode()
            || pem != existing.ssl_root_cert;

        let record = repo::update(
            self.db,
            &NewConnection {
                id,
                user_id,
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
            self.db,
            AuditEvent::new(Some(user_id), "connection.updated")
                .connection(id)
                .detail(json!({
                    "target_changed": reset_status,
                    "password_changed": req.password.is_some(),
                    "read_only": record.read_only,
                    "environment": record.environment,
                })),
        )
        .await;
        Ok(ConnectionResponse::from(&record))
    }

    /// Copies a connection, including its credentials, under a free
    /// "<name> (copy)" name. The secret is re-encrypted because the
    /// ciphertext is bound to the connection id.
    pub async fn duplicate(&self, user_id: Uuid, id: Uuid) -> AppResult<ConnectionResponse> {
        let source = self.get_record(user_id, id).await?;
        let secret = self.decrypt_secret(&source)?;
        let name = self.free_copy_name(user_id, &source.name).await?;
        let new_id = Uuid::new_v4();
        let encrypted = self.encrypt_secret(user_id, new_id, &secret.password)?;
        let record = repo::insert(
            self.db,
            &NewConnection {
                id: new_id,
                user_id,
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
        tracing::info!(connection_id = %record.id, source_id = %id, %user_id, "connection duplicated");
        audit::record(
            self.db,
            AuditEvent::new(Some(user_id), "connection.created")
                .connection(record.id)
                .detail(json!({ "name": record.name, "duplicated_from": id })),
        )
        .await;
        Ok(ConnectionResponse::from(&record))
    }

    async fn free_copy_name(&self, user_id: Uuid, base: &str) -> AppResult<String> {
        // Leave room for the suffix within the 100-character limit.
        let stem: String = base.chars().take(85).collect();
        for n in 1..=50 {
            let candidate = if n == 1 {
                format!("{stem} (copy)")
            } else {
                format!("{stem} (copy {n})")
            };
            if !repo::name_exists(self.db, user_id, &candidate).await? {
                return Ok(candidate);
            }
        }
        Err(AppError::Conflict(
            "too many copies of this connection; rename some first".into(),
        ))
    }

    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_for_user(self.db, user_id, id).await? {
            tracing::info!(connection_id = %id, %user_id, "connection deleted");
            audit::record(
                self.db,
                AuditEvent::new(Some(user_id), "connection.deleted").target("connection", id),
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
        let outcome =
            target_db::test_connection(self.config, &TargetSpec::from_record(&record, &secret))
                .await;
        match outcome {
            Ok(result) => {
                let latency = i32::try_from(result.latency_ms).unwrap_or(i32::MAX);
                repo::record_status(
                    self.db,
                    user_id,
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
                    self.db,
                    user_id,
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

    // -- groups -----------------------------------------------------------

    pub async fn groups(&self, user_id: Uuid) -> AppResult<Vec<ConnectionGroup>> {
        Ok(repo::list_groups(self.db, user_id).await?)
    }

    pub async fn create_group(
        &self,
        user_id: Uuid,
        req: &ConnectionGroupRequest,
    ) -> AppResult<ConnectionGroup> {
        let position = match req.position {
            Some(p) => p,
            None => repo::next_group_position(self.db, user_id).await?,
        };
        repo::insert_group(self.db, user_id, req.name.trim(), position)
            .await
            .map_err(group_write_error)
    }

    pub async fn update_group(
        &self,
        user_id: Uuid,
        id: Uuid,
        req: &ConnectionGroupRequest,
    ) -> AppResult<ConnectionGroup> {
        repo::update_group(self.db, user_id, id, req.name.trim(), req.position)
            .await
            .map_err(group_write_error)?
            .ok_or(AppError::NotFound("connection group"))
    }

    /// Deletes a group; its connections become ungrouped.
    pub async fn delete_group(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_group(self.db, user_id, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("connection group"))
        }
    }
}

/// Unique-name conflicts get a specific message; a foreign key violation
/// means the group does not exist for this user and is reported as 404.
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

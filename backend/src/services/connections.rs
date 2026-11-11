//! Business logic for user-owned database connections.

use sqlx::PgPool;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    database::connections::{self as repo, NewConnection},
    errors::{AppError, AppResult},
    models::connection::{
        ConnectionRecord, ConnectionResponse, ConnectionSecret, CreateConnectionRequest,
        UpdateConnectionRequest,
    },
    state::AppState,
    utils::crypto::CredentialCipher,
};

pub struct ConnectionService<'a> {
    db: &'a PgPool,
    cipher: &'a CredentialCipher,
}

/// Associated data binding a ciphertext to its owner and row.
fn credentials_aad(user_id: Uuid, connection_id: Uuid) -> Vec<u8> {
    format!("querylab/connection/v1/{user_id}/{connection_id}").into_bytes()
}

impl<'a> ConnectionService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self {
            db: &state.db,
            cipher: &state.cipher,
        }
    }

    pub fn from_parts(db: &'a PgPool, cipher: &'a CredentialCipher) -> Self {
        Self { db, cipher }
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
        let id = Uuid::new_v4();
        let encrypted = self.encrypt_secret(user_id, id, &req.password)?;
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
            },
        )
        .await
        .map_err(conflict_as_name_taken)?;
        tracing::info!(connection_id = %record.id, %user_id, "connection created");
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
        let record = repo::update(
            self.db,
            &NewConnection {
                id,
                user_id,
                name,
                host,
                port: req.port.unwrap_or(existing.port()),
                database_name: req.database.as_deref().unwrap_or(&existing.database_name),
                username: req.username.as_deref().unwrap_or(&existing.username),
                ssl_mode: req
                    .ssl_mode
                    .map(|m| m.as_str())
                    .unwrap_or(existing.ssl_mode().as_str()),
                read_only: req.read_only.unwrap_or(existing.read_only),
                encrypted_credentials: &encrypted,
            },
        )
        .await
        .map_err(conflict_as_name_taken)?
        .ok_or(AppError::NotFound("connection"))?;
        Ok(ConnectionResponse::from(&record))
    }

    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        if repo::delete_for_user(self.db, user_id, id).await? {
            tracing::info!(connection_id = %id, %user_id, "connection deleted");
            Ok(())
        } else {
            Err(AppError::NotFound("connection"))
        }
    }

    pub async fn mark_connected(&self, user_id: Uuid, id: Uuid) -> AppResult<()> {
        repo::touch_last_connected(self.db, user_id, id).await?;
        Ok(())
    }
}

fn conflict_as_name_taken(err: sqlx::Error) -> AppError {
    match AppError::from(err) {
        AppError::Conflict(_) => {
            AppError::Conflict("a connection with this name already exists".into())
        }
        other => other,
    }
}

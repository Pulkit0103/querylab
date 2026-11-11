use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, utils::crypto::CredentialCipher};

/// Shared application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub cipher: Arc<CredentialCipher>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool) -> Self {
        let cipher = CredentialCipher::new(&config.encryption_key, &config.previous_encryption_keys);
        Self {
            config: Arc::new(config),
            db,
            cipher: Arc::new(cipher),
        }
    }
}

use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::{config::Config, middleware::rate_limit::RateLimiter, utils::crypto::CredentialCipher};

/// Shared application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub cipher: Arc<CredentialCipher>,
    /// Throttles login and registration attempts.
    pub auth_limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool) -> Self {
        let cipher =
            CredentialCipher::new(&config.encryption_key, &config.previous_encryption_keys);
        let auth_limiter =
            RateLimiter::new(config.auth_rate_limit_per_minute, Duration::from_secs(60));
        Self {
            config: Arc::new(config),
            db,
            cipher: Arc::new(cipher),
            auth_limiter: Arc::new(auth_limiter),
        }
    }
}

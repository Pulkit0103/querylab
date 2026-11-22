use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::{
    config::Config,
    middleware::{metrics::Metrics, rate_limit::RateLimiter},
    services::{data_jobs::DataJobs, execution::ExecutionRegistry, mailer::Mailer},
    utils::crypto::CredentialCipher,
};

/// Monitoring refreshes allowed per user, connection and view.
pub const MONITOR_REFRESHES_PER_MINUTE: u32 = 30;

/// Shared application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub cipher: Arc<CredentialCipher>,
    /// Throttles login and registration attempts.
    pub auth_limiter: Arc<RateLimiter>,
    /// Throttles outbound connections to target databases, per user.
    pub connect_limiter: Arc<RateLimiter>,
    /// Throttles query executions, per user.
    pub query_limiter: Arc<RateLimiter>,
    /// Throttles monitoring refreshes, per user, connection and view.
    pub monitor_limiter: Arc<RateLimiter>,
    /// Running executions (cancellation and concurrency limits).
    pub executions: Arc<ExecutionRegistry>,
    /// Imports and mock data runs in progress.
    pub jobs: Arc<DataJobs>,
    pub mailer: Arc<Mailer>,
    pub metrics: Arc<Metrics>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool) -> Self {
        let cipher =
            CredentialCipher::new(&config.encryption_key, &config.previous_encryption_keys);
        let minute = Duration::from_secs(60);
        let auth_limiter = RateLimiter::new(config.auth_rate_limit_per_minute, minute);
        let connect_limiter = RateLimiter::new(config.target_connect_rate_limit_per_minute, minute);
        let query_limiter = RateLimiter::new(config.query_rate_limit_per_minute, minute);
        // The UI refreshes every 5 s at most; leave room for manual refreshes.
        let monitor_limiter = RateLimiter::new(MONITOR_REFRESHES_PER_MINUTE, minute);
        let jobs = Arc::new(DataJobs::new(config.import_memory_budget));
        let mailer = Arc::new(Mailer::new(config.email.clone()));
        Self {
            config: Arc::new(config),
            db,
            cipher: Arc::new(cipher),
            auth_limiter: Arc::new(auth_limiter),
            connect_limiter: Arc::new(connect_limiter),
            query_limiter: Arc::new(query_limiter),
            monitor_limiter: Arc::new(monitor_limiter),
            executions: Arc::new(ExecutionRegistry::default()),
            jobs,
            mailer,
            metrics: Arc::new(Metrics::default()),
        }
    }
}

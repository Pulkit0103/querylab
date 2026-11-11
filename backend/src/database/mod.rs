//! Application metadata database (PostgreSQL) access.

use std::time::Duration;

use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::Config;

/// Creates the metadata connection pool. Connections are established lazily so
/// the API can start (and report a degraded health status) even if the
/// database is temporarily unavailable.
pub fn create_pool(config: &Config) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .connect_lazy(&config.database_url)
}

/// Lightweight connectivity probe used by health checks. Bounded to two
/// seconds so health endpoints stay responsive when the database is down.
pub async fn ping(pool: &PgPool) -> bool {
    let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool);
    matches!(
        tokio::time::timeout(Duration::from_secs(2), probe).await,
        Ok(Ok(1))
    )
}

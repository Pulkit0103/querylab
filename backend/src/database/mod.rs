//! Application metadata database (PostgreSQL): pool, migrations and
//! repositories. Repositories contain SQL only; business rules live in
//! `services`. All statements are parameterised.

pub mod connections;
pub mod users;

use std::time::Duration;

use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool};

use crate::config::Config;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

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

/// Applies pending migrations, retrying while the database comes up (e.g. a
/// container starting alongside the API).
pub async fn migrate(pool: &PgPool, attempts: u32) -> Result<(), sqlx::migrate::MigrateError> {
    let mut delay = Duration::from_millis(500);
    let mut attempt = 1;
    loop {
        match MIGRATOR.run(pool).await {
            Ok(()) => return Ok(()),
            Err(err) if attempt < attempts && is_transient(&err) => {
                tracing::warn!(attempt, error = %err, "migration attempt failed; retrying");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

fn is_transient(err: &sqlx::migrate::MigrateError) -> bool {
    matches!(
        err,
        sqlx::migrate::MigrateError::Execute(sqlx::Error::Io(_))
            | sqlx::migrate::MigrateError::Execute(sqlx::Error::PoolTimedOut)
            | sqlx::migrate::MigrateError::Execute(sqlx::Error::Tls(_))
    )
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

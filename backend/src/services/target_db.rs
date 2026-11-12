//! Dedicated, short-lived connections to user-owned target databases.
//!
//! User SQL never runs on the metadata pool. Each operation opens its own
//! connection with:
//! * `new_without_pgpass()` so the server's `PG*` environment variables and
//!   `.pgpass` file can never leak into a user connection,
//! * statement logging disabled so user SQL is not written to logs,
//! * `default_transaction_read_only=on` for read-only connections,
//! * an outbound network policy check and a connect timeout.

use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::{
    postgres::{PgConnectOptions, PgSslMode},
    ConnectOptions, Connection, PgConnection, Row,
};

use crate::{
    config::Config,
    errors::{AppError, AppResult},
    models::connection::{ConnectionRecord, ConnectionSecret, CreateConnectionRequest, SslMode},
    utils::network,
};

/// Everything needed to open a connection to a target database.
pub struct TargetSpec<'a> {
    pub host: &'a str,
    pub port: u16,
    pub database: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub ssl_mode: SslMode,
    pub read_only: bool,
}

impl<'a> TargetSpec<'a> {
    pub fn from_record(record: &'a ConnectionRecord, secret: &'a ConnectionSecret) -> Self {
        Self {
            host: &record.host,
            port: record.port(),
            database: &record.database_name,
            username: &record.username,
            password: &secret.password,
            ssl_mode: record.ssl_mode(),
            read_only: record.read_only,
        }
    }

    pub fn from_request(req: &'a CreateConnectionRequest) -> Self {
        Self {
            host: req.host.trim(),
            port: req.port,
            database: &req.database,
            username: &req.username,
            password: &req.password,
            ssl_mode: req.ssl_mode,
            read_only: req.read_only,
        }
    }
}

fn pg_ssl_mode(mode: SslMode) -> PgSslMode {
    match mode {
        SslMode::Disable => PgSslMode::Disable,
        SslMode::Prefer => PgSslMode::Prefer,
        SslMode::Require => PgSslMode::Require,
        SslMode::VerifyCa => PgSslMode::VerifyCa,
        SslMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

/// Opens a dedicated connection after enforcing the network policy.
pub async fn connect(config: &Config, spec: &TargetSpec<'_>) -> AppResult<PgConnection> {
    network::check_destination(spec.host, spec.port, config.allow_private_networks).await?;

    let host = spec
        .host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(spec.host);

    let mut options = PgConnectOptions::new_without_pgpass()
        .host(host)
        .port(spec.port)
        .database(spec.database)
        .username(spec.username)
        .password(spec.password)
        .ssl_mode(pg_ssl_mode(spec.ssl_mode))
        .application_name("querylab")
        .statement_cache_capacity(0)
        .disable_statement_logging();
    if spec.read_only {
        options = options.options([("default_transaction_read_only", "on")]);
    }

    let timeout = config.target_connect_timeout;
    match tokio::time::timeout(timeout, options.connect()).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(err)) => Err(map_connect_error(err, spec.host, spec.port)),
        Err(_) => Err(AppError::TargetUnavailable(format!(
            "timed out connecting to {}:{} after {}s",
            spec.host,
            spec.port,
            timeout.as_secs()
        ))),
    }
}

/// Converts driver errors into user-facing messages that never include the
/// password or the server's internal details.
pub fn map_connect_error(err: sqlx::Error, host: &str, port: u16) -> AppError {
    match err {
        sqlx::Error::Database(db) => AppError::TargetUnavailable(db.message().to_string()),
        sqlx::Error::Io(io) => {
            let reason = match io.kind() {
                std::io::ErrorKind::ConnectionRefused => "connection refused".to_string(),
                std::io::ErrorKind::TimedOut => "connection timed out".to_string(),
                std::io::ErrorKind::ConnectionReset => "connection reset by peer".to_string(),
                std::io::ErrorKind::NotFound => "host not found".to_string(),
                _ => io.to_string(),
            };
            AppError::TargetUnavailable(format!("could not connect to {host}:{port}: {reason}"))
        }
        sqlx::Error::Tls(e) => AppError::TargetUnavailable(format!("TLS negotiation failed: {e}")),
        sqlx::Error::Protocol(_) => AppError::TargetUnavailable(
            "unexpected response from server (is this a PostgreSQL server?)".into(),
        ),
        other => {
            tracing::warn!(error = %other, "unexpected target connection error");
            AppError::TargetUnavailable(format!("could not connect to {host}:{port}"))
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub server_version: String,
    pub database: String,
    pub user: String,
    pub is_superuser: bool,
    /// Whether new transactions on this connection are read-only.
    pub read_only: bool,
    pub warnings: Vec<String>,
}

/// Connects and gathers server facts. Never modifies the target.
pub async fn test_connection(
    config: &Config,
    spec: &TargetSpec<'_>,
) -> AppResult<ConnectionTestResult> {
    let started = Instant::now();
    let mut conn = connect(config, spec).await?;
    let row = sqlx::query(
        "SELECT current_setting('server_version') AS version, current_database() AS db, \
                current_user AS usr, \
                COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), false) AS su, \
                current_setting('transaction_read_only') = 'on' AS ro",
    )
    .fetch_one(&mut conn)
    .await
    .map_err(|e| map_connect_error(e, spec.host, spec.port))?;
    let latency_ms = started.elapsed().as_millis() as u64;
    let _ = tokio::time::timeout(Duration::from_secs(2), conn.close()).await;

    let is_superuser: bool = row.try_get("su").unwrap_or(false);
    let read_only: bool = row.try_get("ro").unwrap_or(false);
    let mut warnings = Vec::new();
    if is_superuser {
        warnings.push(
            "Connected as a superuser. Use a least-privilege role with read-only grants.".into(),
        );
    }
    if !spec.read_only {
        warnings.push(
            "Read-write mode is enabled: statements can modify data on this database.".into(),
        );
    }
    if matches!(spec.ssl_mode, SslMode::Disable | SslMode::Prefer) {
        warnings.push(
            "Traffic may be unencrypted. Prefer `require` or `verify-full` for remote databases."
                .into(),
        );
    }

    Ok(ConnectionTestResult {
        ok: true,
        latency_ms,
        server_version: row.try_get("version").unwrap_or_default(),
        database: row.try_get("db").unwrap_or_default(),
        user: row.try_get("usr").unwrap_or_default(),
        is_superuser,
        read_only,
        warnings,
    })
}

//! Runtime configuration loaded from environment variables.
//!
//! Every value has a safe default for local development except secrets, which
//! must be provided explicitly. Production refuses the published dev key.

use std::{env, net::SocketAddr, time::Duration};

use thiserror::Error;

use crate::utils::crypto::EncryptionKey;

/// Development-only key published in `.env.example`. Never valid in production.
pub const DEV_ENCRYPTION_KEY: &str = "ngSxD0iS9jusyKYm69L1zh9617mM1Y9WJ8TeX3N0TrU=";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable `{0}`")]
    Missing(&'static str),
    #[error("invalid value for `{name}`: {reason}")]
    Invalid { name: &'static str, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn is_production(&self) -> bool {
        matches!(self, Environment::Production)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub environment: Environment,
    pub bind_addr: SocketAddr,
    /// Connection string for the application metadata database.
    pub database_url: String,
    pub database_max_connections: u32,
    pub run_migrations: bool,
    /// Origins allowed to call the API directly (CORS). Same-origin proxying
    /// through the frontend does not require CORS.
    pub cors_origins: Vec<String>,
    pub log_format: LogFormat,
    pub request_timeout: Duration,
    /// Key used to encrypt new credentials.
    pub encryption_key: EncryptionKey,
    /// Older keys still accepted for decryption during rotation.
    pub previous_encryption_keys: Vec<EncryptionKey>,
    /// Sliding session lifetime.
    pub session_ttl: Duration,
    /// Hard cap on a session's lifetime regardless of activity.
    pub session_absolute_max: Duration,
    /// Mark the session cookie `Secure` (and use the `__Host-` prefix).
    pub cookie_secure: bool,
    /// Number of trusted reverse proxies appending to `X-Forwarded-For`.
    pub trusted_proxy_hops: usize,
    /// Login/registration attempts allowed per minute per IP and per account.
    pub auth_rate_limit_per_minute: u32,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let environment = match env::var("APP_ENV").unwrap_or_default().as_str() {
            "production" | "prod" => Environment::Production,
            _ => Environment::Development,
        };

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = parse_or("PORT", 8080)?;
        let bind_addr: SocketAddr =
            format!("{host}:{port}")
                .parse()
                .map_err(|e: std::net::AddrParseError| ConfigError::Invalid {
                    name: "HOST/PORT",
                    reason: e.to_string(),
                })?;

        let database_url =
            env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;

        let cors_origins = env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .split(',')
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let log_format = match env::var("LOG_FORMAT").unwrap_or_default().as_str() {
            "json" => LogFormat::Json,
            "pretty" => LogFormat::Pretty,
            _ if environment.is_production() => LogFormat::Json,
            _ => LogFormat::Pretty,
        };

        let (encryption_key, previous_encryption_keys) = load_encryption_keys(&environment)?;
        let cookie_secure = parse_or("COOKIE_SECURE", environment.is_production())?;

        Ok(Self {
            environment,
            bind_addr,
            database_url,
            database_max_connections: parse_or("DATABASE_MAX_CONNECTIONS", 10)?,
            run_migrations: parse_or("RUN_MIGRATIONS", true)?,
            cors_origins,
            log_format,
            request_timeout: Duration::from_secs(parse_or("REQUEST_TIMEOUT_SECS", 120)?),
            encryption_key,
            previous_encryption_keys,
            session_ttl: Duration::from_secs(parse_or::<u64>("SESSION_TTL_HOURS", 168)? * 3600),
            session_absolute_max: Duration::from_secs(
                parse_or::<u64>("SESSION_MAX_AGE_DAYS", 30)? * 86_400,
            ),
            cookie_secure,
            trusted_proxy_hops: parse_or("TRUSTED_PROXY_HOPS", 0)?,
            auth_rate_limit_per_minute: parse_or("AUTH_RATE_LIMIT_PER_MINUTE", 10)?,
        })
    }

    /// Defaults for tests: development mode with the dev encryption key.
    #[doc(hidden)]
    pub fn for_tests(database_url: &str) -> Self {
        Self {
            environment: Environment::Development,
            bind_addr: "127.0.0.1:0".parse().expect("valid addr"),
            database_url: database_url.to_string(),
            database_max_connections: 5,
            run_migrations: true,
            cors_origins: vec!["http://localhost:3000".into()],
            log_format: LogFormat::Pretty,
            request_timeout: Duration::from_secs(30),
            encryption_key: EncryptionKey::from_base64(1, DEV_ENCRYPTION_KEY)
                .expect("dev key is valid"),
            previous_encryption_keys: Vec::new(),
            session_ttl: Duration::from_secs(7 * 86_400),
            session_absolute_max: Duration::from_secs(30 * 86_400),
            cookie_secure: false,
            trusted_proxy_hops: 0,
            auth_rate_limit_per_minute: 1000,
        }
    }
}

fn load_encryption_keys(
    environment: &Environment,
) -> Result<(EncryptionKey, Vec<EncryptionKey>), ConfigError> {
    let raw = env::var("ENCRYPTION_KEY").map_err(|_| ConfigError::Missing("ENCRYPTION_KEY"))?;
    if environment.is_production() && raw.trim() == DEV_ENCRYPTION_KEY {
        return Err(ConfigError::Invalid {
            name: "ENCRYPTION_KEY",
            reason: "the development key must not be used in production".into(),
        });
    }
    let key_id: u8 = parse_or("ENCRYPTION_KEY_ID", 1)?;
    let primary = EncryptionKey::from_base64(key_id, &raw).map_err(|e| ConfigError::Invalid {
        name: "ENCRYPTION_KEY",
        reason: e.to_string(),
    })?;

    // Format: "<id>:<base64>,<id>:<base64>"
    let mut previous = Vec::new();
    if let Ok(list) = env::var("ENCRYPTION_PREVIOUS_KEYS") {
        for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (id, key) = entry.split_once(':').ok_or(ConfigError::Invalid {
                name: "ENCRYPTION_PREVIOUS_KEYS",
                reason: "entries must look like `<id>:<base64 key>`".into(),
            })?;
            let id: u8 = id.trim().parse().map_err(|_| ConfigError::Invalid {
                name: "ENCRYPTION_PREVIOUS_KEYS",
                reason: format!("invalid key id `{id}`"),
            })?;
            if id == key_id {
                return Err(ConfigError::Invalid {
                    name: "ENCRYPTION_PREVIOUS_KEYS",
                    reason: format!("key id {id} is already the primary key id"),
                });
            }
            previous.push(EncryptionKey::from_base64(id, key).map_err(|e| {
                ConfigError::Invalid {
                    name: "ENCRYPTION_PREVIOUS_KEYS",
                    reason: e.to_string(),
                }
            })?);
        }
    }
    Ok((primary, previous))
}

/// Parses an optional environment variable, falling back to `default` when unset.
pub(crate) fn parse_or<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(raw) if !raw.trim().is_empty() => {
            raw.trim()
                .parse()
                .map_err(|e: T::Err| ConfigError::Invalid {
                    name,
                    reason: e.to_string(),
                })
        }
        _ => Ok(default),
    }
}

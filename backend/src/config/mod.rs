//! Runtime configuration loaded from environment variables.
//!
//! Every value has a safe default for local development except secrets, which
//! must be provided explicitly in production.

use std::{env, net::SocketAddr, time::Duration};

use thiserror::Error;

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

#[derive(Clone, Debug)]
pub struct Config {
    pub environment: Environment,
    pub bind_addr: SocketAddr,
    /// Connection string for the application metadata database.
    pub database_url: String,
    pub database_max_connections: u32,
    /// Origins allowed to call the API directly (CORS). Same-origin proxying
    /// through the frontend does not require CORS.
    pub cors_origins: Vec<String>,
    pub log_format: LogFormat,
    pub request_timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
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

        Ok(Self {
            environment,
            bind_addr,
            database_url,
            database_max_connections: parse_or("DATABASE_MAX_CONNECTIONS", 10)?,
            cors_origins,
            log_format,
            request_timeout: Duration::from_secs(parse_or("REQUEST_TIMEOUT_SECS", 120)?),
        })
    }
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

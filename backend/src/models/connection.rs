use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    errors::AppError,
    utils::validation::{Validate, Validator},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
            SslMode::VerifyCa => "verify-ca",
            SslMode::VerifyFull => "verify-full",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "disable" => SslMode::Disable,
            "prefer" => SslMode::Prefer,
            "require" => SslMode::Require,
            "verify-ca" => SslMode::VerifyCa,
            "verify-full" => SslMode::VerifyFull,
            _ => return None,
        })
    }
}

/// Row in `database_connections`. Never serialised to clients.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConnectionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub database_type: String,
    pub host: String,
    pub port: i32,
    pub database_name: String,
    pub username: String,
    pub ssl_mode: String,
    pub read_only: bool,
    pub encrypted_credentials: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_connected_at: Option<DateTime<Utc>>,
}

impl ConnectionRecord {
    pub fn ssl_mode(&self) -> SslMode {
        SslMode::parse(&self.ssl_mode).unwrap_or_default()
    }

    pub fn port(&self) -> u16 {
        u16::try_from(self.port).unwrap_or(5432)
    }
}

/// Public representation of a connection. Contains no secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionResponse {
    pub id: Uuid,
    pub name: String,
    pub database_type: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub ssl_mode: SslMode,
    pub read_only: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_connected_at: Option<DateTime<Utc>>,
}

impl From<&ConnectionRecord> for ConnectionResponse {
    fn from(r: &ConnectionRecord) -> Self {
        Self {
            id: r.id,
            name: r.name.clone(),
            database_type: r.database_type.clone(),
            host: r.host.clone(),
            port: r.port(),
            database: r.database_name.clone(),
            username: r.username.clone(),
            ssl_mode: r.ssl_mode(),
            read_only: r.read_only,
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_connected_at: r.last_connected_at,
        }
    }
}

/// Secret material stored encrypted in `encrypted_credentials`.
#[derive(Serialize, Deserialize)]
pub struct ConnectionSecret {
    pub password: String,
}

impl Drop for ConnectionSecret {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
    }
}

impl std::fmt::Debug for ConnectionSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConnectionSecret([redacted])")
    }
}

fn default_port() -> u16 {
    5432
}

fn default_read_only() -> bool {
    true
}

#[derive(Deserialize)]
pub struct CreateConnectionRequest {
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub database: String,
    pub username: String,
    #[serde(default)]
    pub password: Zeroizing<String>,
    #[serde(default)]
    pub ssl_mode: SslMode,
    #[serde(default = "default_read_only")]
    pub read_only: bool,
}

/// Partial update. `password: None` keeps the stored password.
#[derive(Deserialize, Default)]
pub struct UpdateConnectionRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<Zeroizing<String>>,
    pub ssl_mode: Option<SslMode>,
    pub read_only: Option<bool>,
}

/// Hostname or IP literal: letters, digits, dots, hyphens, colons (IPv6) and
/// brackets. Rejects URLs, credentials, paths and whitespace.
pub fn is_valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let trimmed = host.strip_prefix('[').and_then(|h| h.strip_suffix(']'));
    if let Some(inner) = trimmed {
        return inner.parse::<std::net::Ipv6Addr>().is_ok();
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

fn validate_fields(
    v: &mut Validator,
    name: Option<&str>,
    host: Option<&str>,
    port: Option<u16>,
    database: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) {
    if let Some(name) = name {
        v.length("name", name.trim(), 1, 100)
            .no_control_chars("name", name);
    }
    if let Some(host) = host {
        v.check(
            is_valid_host(host.trim()),
            "host",
            "must be a hostname or IP address (no scheme, port or path)",
        );
    }
    if let Some(port) = port {
        v.check(port > 0, "port", "must be between 1 and 65535");
    }
    if let Some(database) = database {
        v.length("database", database, 1, 63)
            .no_control_chars("database", database);
    }
    if let Some(username) = username {
        v.length("username", username, 1, 63)
            .no_control_chars("username", username);
    }
    if let Some(password) = password {
        v.check(
            password.len() <= 1024,
            "password",
            "must be at most 1024 bytes",
        )
        .check(
            !password.contains('\0'),
            "password",
            "must not contain NUL bytes",
        );
    }
}

impl Validate for CreateConnectionRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_fields(
            &mut v,
            Some(&self.name),
            Some(&self.host),
            Some(self.port),
            Some(&self.database),
            Some(&self.username),
            Some(&self.password),
        );
        v.finish()
    }
}

impl Validate for UpdateConnectionRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        validate_fields(
            &mut v,
            self.name.as_deref(),
            self.host.as_deref(),
            self.port,
            self.database.as_deref(),
            self.username.as_deref(),
            self.password.as_ref().map(|p| p.as_str()),
        );
        v.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_validation() {
        for ok in [
            "localhost",
            "db.example.com",
            "ep-cool-name-123.us-east-2.aws.neon.tech",
            "10.0.0.1",
            "::1",
            "[2001:db8::1]",
        ] {
            assert!(is_valid_host(ok), "{ok}");
        }
        for bad in [
            "",
            "http://db.example.com",
            "db.example.com:5432",
            "user@db",
            "db/x",
            "db example",
            "-bad.com",
            "a..b",
            "db.example.com?x=1",
        ] {
            assert!(!is_valid_host(bad), "{bad}");
        }
    }

    #[test]
    fn create_request_defaults_and_validation() {
        let req: CreateConnectionRequest = serde_json::from_value(serde_json::json!({
            "name": "Prod", "host": "db.example.com", "database": "app", "username": "reader"
        }))
        .unwrap();
        assert_eq!(req.port, 5432);
        assert!(req.read_only);
        assert_eq!(req.ssl_mode, SslMode::Prefer);
        assert!(req.validate().is_ok());

        let bad: CreateConnectionRequest = serde_json::from_value(serde_json::json!({
            "name": " ", "host": "http://x", "database": "", "username": "u", "port": 0
        }))
        .unwrap();
        match bad.validate().unwrap_err() {
            AppError::Validation(f) => {
                let fields: Vec<_> = f.iter().map(|e| e.field.as_str()).collect();
                assert_eq!(fields, ["name", "host", "port", "database"]);
            }
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn ssl_mode_round_trip() {
        for m in [
            SslMode::Disable,
            SslMode::Prefer,
            SslMode::Require,
            SslMode::VerifyCa,
            SslMode::VerifyFull,
        ] {
            assert_eq!(SslMode::parse(m.as_str()), Some(m));
            let json = serde_json::to_string(&m).unwrap();
            assert_eq!(json, format!("\"{}\"", m.as_str()));
        }
    }

    #[test]
    fn secret_debug_is_redacted() {
        let s = ConnectionSecret {
            password: "hunter2".into(),
        };
        assert!(!format!("{s:?}").contains("hunter2"));
    }
}

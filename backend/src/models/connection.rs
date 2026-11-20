use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    errors::AppError,
    utils::validation::{double_option, Validate, Validator},
};

/// Deployment stage of a connection. Production connections get extra
/// guardrails in the UI and for privileged operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Development => "development",
            Environment::Staging => "staging",
            Environment::Production => "production",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "development" => Environment::Development,
            "staging" => Environment::Staging,
            "production" => Environment::Production,
            _ => return None,
        })
    }
}

/// Outcome of the most recent connection test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Online,
    Unreachable,
    AuthFailed,
    TlsError,
    Error,
}

impl ConnectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionStatus::Online => "online",
            ConnectionStatus::Unreachable => "unreachable",
            ConnectionStatus::AuthFailed => "auth_failed",
            ConnectionStatus::TlsError => "tls_error",
            ConnectionStatus::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "online" => ConnectionStatus::Online,
            "unreachable" => ConnectionStatus::Unreachable,
            "auth_failed" => ConnectionStatus::AuthFailed,
            "tls_error" => ConnectionStatus::TlsError,
            "error" => ConnectionStatus::Error,
            _ => return None,
        })
    }
}

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
    pub environment: String,
    pub group_id: Option<Uuid>,
    pub color: Option<String>,
    pub ssl_root_cert: Option<String>,
    pub last_status: Option<String>,
    pub last_status_at: Option<DateTime<Utc>>,
    pub last_latency_ms: Option<i32>,
    pub last_error: Option<String>,
}

impl ConnectionRecord {
    pub fn ssl_mode(&self) -> SslMode {
        SslMode::parse(&self.ssl_mode).unwrap_or_default()
    }

    pub fn environment(&self) -> Environment {
        Environment::parse(&self.environment).unwrap_or_default()
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
    pub environment: Environment,
    pub group_id: Option<Uuid>,
    pub color: Option<String>,
    /// PEM CA certificate(s) used to verify the server. Not secret.
    pub ssl_root_cert: Option<String>,
    /// `null` until the connection has been tested.
    pub last_status: Option<ConnectionStatus>,
    pub last_status_at: Option<DateTime<Utc>>,
    pub last_latency_ms: Option<i32>,
    pub last_error: Option<String>,
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
            environment: r.environment(),
            group_id: r.group_id,
            color: r.color.clone(),
            ssl_root_cert: r.ssl_root_cert.clone(),
            last_status: r.last_status.as_deref().and_then(ConnectionStatus::parse),
            last_status_at: r.last_status_at,
            last_latency_ms: r.last_latency_ms,
            last_error: r.last_error.clone(),
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
    #[serde(default)]
    pub environment: Environment,
    pub group_id: Option<Uuid>,
    pub color: Option<String>,
    pub ssl_root_cert: Option<String>,
}

/// Partial update. `password: None` keeps the stored password. For the
/// nullable fields an explicit `null` clears the value and an absent field
/// keeps it.
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
    pub environment: Option<Environment>,
    #[serde(default, deserialize_with = "double_option")]
    pub group_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub ssl_root_cert: Option<Option<String>>,
}

/// `#rrggbb` (case-insensitive); stored lowercase.
pub fn is_valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Maximum accepted PEM bundle size.
pub const MAX_SSL_ROOT_CERT_BYTES: usize = 64 * 1024;

/// Structural check for a PEM certificate bundle. Full parsing happens when
/// the TLS connection is made, which reports a clear error for bad input.
pub fn is_valid_pem_bundle(pem: &str) -> bool {
    let pem = pem.trim();
    let begins = pem.matches("-----BEGIN CERTIFICATE-----").count();
    let ends = pem.matches("-----END CERTIFICATE-----").count();
    !pem.is_empty()
        && pem.len() <= MAX_SSL_ROOT_CERT_BYTES
        && pem.is_ascii()
        && pem.starts_with("-----BEGIN CERTIFICATE-----")
        && pem.ends_with("-----END CERTIFICATE-----")
        && begins >= 1
        && begins == ends
}

fn validate_extras(v: &mut Validator, color: Option<&str>, ssl_root_cert: Option<&str>) {
    if let Some(color) = color {
        v.check(
            is_valid_color(color),
            "color",
            "must be a hex colour like #3b82f6",
        );
    }
    if let Some(pem) = ssl_root_cert {
        v.check(
            is_valid_pem_bundle(pem),
            "ssl_root_cert",
            "must be one or more PEM certificates (-----BEGIN CERTIFICATE----- ...), at most 64 KiB",
        );
    }
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
        validate_extras(&mut v, self.color.as_deref(), self.ssl_root_cert.as_deref());
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
        validate_extras(
            &mut v,
            self.color.as_ref().and_then(|c| c.as_deref()),
            self.ssl_root_cert.as_ref().and_then(|c| c.as_deref()),
        );
        v.finish()
    }
}

// ---------------------------------------------------------------------------
// Connection groups
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConnectionGroup {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub connection_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectionGroupRequest {
    pub name: String,
    pub position: Option<i32>,
}

impl Validate for ConnectionGroupRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.length("name", self.name.trim(), 1, 100)
            .no_control_chars("name", &self.name);
        if let Some(p) = self.position {
            v.check(
                (0..=10_000).contains(&p),
                "position",
                "must be between 0 and 10000",
            );
        }
        v.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str =
        "-----BEGIN CERTIFICATE-----\nMIIBszCCAVmgAwIBAgIU\n-----END CERTIFICATE-----";

    #[test]
    fn colour_and_pem_validation() {
        assert!(is_valid_color("#3B82f6"));
        for bad in ["3b82f6", "#3b82f", "#3b82fg", "red", "#3b82f6aa"] {
            assert!(!is_valid_color(bad), "{bad}");
        }
        assert!(is_valid_pem_bundle(CERT));
        assert!(is_valid_pem_bundle(&format!("{CERT}\n{CERT}\n")));
        assert!(!is_valid_pem_bundle(
            "-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY-----"
        ));
        assert!(!is_valid_pem_bundle("-----BEGIN CERTIFICATE-----\nabc"));
        assert!(!is_valid_pem_bundle(""));
        assert!(!is_valid_pem_bundle(
            &"A".repeat(MAX_SSL_ROOT_CERT_BYTES + 1)
        ));
    }

    #[test]
    fn update_distinguishes_clear_from_keep() {
        let keep: UpdateConnectionRequest = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
        assert!(keep.group_id.is_none() && keep.color.is_none());
        let clear: UpdateConnectionRequest =
            serde_json::from_str(r#"{"group_id":null,"color":null,"ssl_root_cert":null}"#).unwrap();
        assert_eq!(clear.group_id, Some(None));
        assert_eq!(clear.color, Some(None));
        assert_eq!(clear.ssl_root_cert, Some(None));
        let bad: UpdateConnectionRequest = serde_json::from_str(r##"{"color":"blue"}"##).unwrap();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn environment_and_status_round_trip() {
        for e in [
            Environment::Development,
            Environment::Staging,
            Environment::Production,
        ] {
            assert_eq!(Environment::parse(e.as_str()), Some(e));
            assert_eq!(
                serde_json::to_string(&e).unwrap(),
                format!("\"{}\"", e.as_str())
            );
        }
        for s in [
            ConnectionStatus::Online,
            ConnectionStatus::Unreachable,
            ConnectionStatus::AuthFailed,
            ConnectionStatus::TlsError,
            ConnectionStatus::Error,
        ] {
            assert_eq!(ConnectionStatus::parse(s.as_str()), Some(s));
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
        let req: CreateConnectionRequest = serde_json::from_value(serde_json::json!({
            "name": "x", "host": "db.example.com", "database": "d", "username": "u"
        }))
        .unwrap();
        assert_eq!(req.environment, Environment::Development);
    }

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

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub totp_enabled_at: Option<DateTime<Utc>>,
}

/// Public user representation (no password hash).
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub two_factor_enabled: bool,
}

impl From<&UserRecord> for UserResponse {
    fn from(u: &UserRecord) -> Self {
        Self {
            id: u.id,
            email: u.email.clone(),
            display_name: u.display_name.clone(),
            created_at: u.created_at,
            two_factor_enabled: u.totp_enabled_at.is_some(),
        }
    }
}

//! Accounts and sessions.
//!
//! Sessions are opaque 256-bit random tokens. Only their SHA-256 digest is
//! stored, so a database leak does not reveal usable tokens. Expiry slides on
//! activity but is capped by an absolute lifetime.

use std::sync::OnceLock;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    config::Config,
    database::{sessions, users},
    errors::{AppError, AppResult},
    models::{
        auth::{LoginRequest, RegisterRequest},
        user::UserResponse,
    },
    state::AppState,
};

/// Refresh `last_seen_at`/expiry at most this often per session.
const TOUCH_INTERVAL_SECS: i64 = 300;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
}

pub struct IssuedSession {
    pub token: Zeroizing<String>,
    pub expires_at: DateTime<Utc>,
}

pub fn generate_token() -> Zeroizing<String> {
    use argon2::password_hash::rand_core::RngCore;
    let mut bytes = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(bytes.as_mut());
    Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref()))
}

pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

async fn hash_password(password: Zeroizing<String>) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::internal(format!("password hashing failed: {e}")))
    })
    .await
    .map_err(|e| AppError::internal(format!("hashing task failed: {e}")))?
}

async fn verify_password(password: Zeroizing<String>, hash: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash)
            .map_err(|e| AppError::internal(format!("stored password hash is invalid: {e}")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|e| AppError::internal(format!("verification task failed: {e}")))?
}

/// Hash verified when the email is unknown, so response time does not reveal
/// whether an account exists.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"querylab-timing-equaliser", &salt)
            .expect("hashing a constant succeeds")
            .to_string()
    })
}

pub struct AuthService<'a> {
    db: &'a PgPool,
    config: &'a Config,
}

impl<'a> AuthService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self {
            db: &state.db,
            config: &state.config,
        }
    }

    async fn issue_session(
        &self,
        user_id: Uuid,
        user_agent: Option<&str>,
    ) -> AppResult<IssuedSession> {
        let token = generate_token();
        let expires_at = Utc::now() + self.ttl();
        let ua = user_agent.map(|s| s.chars().take(255).collect::<String>());
        sessions::insert(
            self.db,
            Uuid::new_v4(),
            user_id,
            &hash_token(&token),
            expires_at,
            ua.as_deref(),
        )
        .await?;
        Ok(IssuedSession { token, expires_at })
    }

    fn ttl(&self) -> ChronoDuration {
        ChronoDuration::from_std(self.config.session_ttl).unwrap_or(ChronoDuration::days(7))
    }

    fn absolute_max(&self) -> ChronoDuration {
        ChronoDuration::from_std(self.config.session_absolute_max)
            .unwrap_or(ChronoDuration::days(30))
    }

    pub async fn register(
        &self,
        req: RegisterRequest,
        user_agent: Option<&str>,
    ) -> AppResult<(UserResponse, IssuedSession)> {
        let email = req.email.trim().to_string();
        let display_name = req
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let hash = hash_password(req.password).await?;
        let user = users::insert(
            self.db,
            Uuid::new_v4(),
            &email,
            &hash,
            display_name.as_deref(),
        )
        .await
        .map_err(|e| match AppError::from(e) {
            AppError::Conflict(_) => {
                AppError::Conflict("an account with this email already exists".into())
            }
            other => other,
        })?;
        tracing::info!(user_id = %user.id, "user registered");
        let session = self.issue_session(user.id, user_agent).await?;
        Ok((UserResponse::from(&user), session))
    }

    pub async fn login(
        &self,
        req: LoginRequest,
        user_agent: Option<&str>,
    ) -> AppResult<(UserResponse, IssuedSession)> {
        let user = users::find_by_email(self.db, req.email.trim()).await?;
        let Some(user) = user else {
            // Equalise timing with the "wrong password" path.
            let _ = verify_password(req.password, dummy_hash().to_string()).await;
            return Err(AppError::InvalidCredentials);
        };
        if !verify_password(req.password, user.password_hash.clone()).await? {
            tracing::info!(user_id = %user.id, "failed login");
            return Err(AppError::InvalidCredentials);
        }
        let session = self.issue_session(user.id, user_agent).await?;
        tracing::info!(user_id = %user.id, "user logged in");
        Ok((UserResponse::from(&user), session))
    }

    pub async fn logout(&self, session_id: Uuid) -> AppResult<()> {
        sessions::delete(self.db, session_id).await?;
        Ok(())
    }

    /// Resolves a bearer token to a user, extending the session on activity.
    pub async fn authenticate(&self, token: &str) -> AppResult<Option<AuthUser>> {
        // Tokens are 43 base64url chars; reject anything implausible early.
        if token.is_empty() || token.len() > 128 {
            return Ok(None);
        }
        let Some(session) = sessions::find_active(self.db, &hash_token(token)).await? else {
            return Ok(None);
        };

        let now = Utc::now();
        if (now - session.last_seen_at).num_seconds() >= TOUCH_INTERVAL_SECS {
            let hard_limit = session.created_at + self.absolute_max();
            let expires_at = (now + self.ttl()).min(hard_limit);
            sessions::extend(self.db, session.session_id, expires_at).await?;
        }

        Ok(Some(AuthUser {
            user_id: session.user_id,
            session_id: session.session_id,
            email: session.email,
            display_name: session.display_name,
        }))
    }
}

/// Removes expired sessions periodically.
pub fn spawn_session_cleanup(db: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match sessions::delete_expired(&db).await {
                Ok(n) if n > 0 => tracing::info!(removed = n, "expired sessions removed"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "session cleanup failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_url_safe() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(*a, *b);
        assert_eq!(a.len(), 43);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn token_hash_is_stable_sha256() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }

    #[tokio::test]
    async fn password_hash_round_trip() {
        let hash = hash_password(Zeroizing::new("correct horse battery".into()))
            .await
            .unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            verify_password(Zeroizing::new("correct horse battery".into()), hash.clone())
                .await
                .unwrap()
        );
        assert!(!verify_password(Zeroizing::new("wrong".into()), hash)
            .await
            .unwrap());
    }
}

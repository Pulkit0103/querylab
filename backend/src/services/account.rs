//! Account security: password change and reset, sessions, and TOTP
//! two-factor authentication with recovery codes.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    database::{
        account::{self, SessionInfo},
        sessions, users,
    },
    errors::{AppError, AppResult},
    services::{
        audit::{self, AuditEvent},
        auth::{dummy_hash, generate_token, hash_password, hash_token, verify_password, AuthUser},
        mailer::Email,
    },
    state::AppState,
    utils::totp,
};

const ISSUER: &str = "QueryLab";
const RECOVERY_CODES: usize = 10;
const RESET_TTL_MINUTES: i64 = 60;
const RESETS_PER_HOUR: i64 = 3;

fn totp_aad(user_id: Uuid) -> Vec<u8> {
    format!("totp:{user_id}").into_bytes()
}

/// Recovery codes are compared without case, spaces or dashes.
fn normalize_recovery(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn recovery_hash(code: &str) -> Vec<u8> {
    Sha256::digest(normalize_recovery(code).as_bytes()).to_vec()
}

/// `xxxxx-xxxxx` from 50 random bits.
fn new_recovery_code() -> String {
    let mut bytes = [0u8; 7];
    OsRng.fill_bytes(&mut bytes);
    let code = totp::base32_encode(&bytes).to_ascii_lowercase();
    format!("{}-{}", &code[..5], &code[5..10])
}

fn unix_now() -> u64 {
    Utc::now().timestamp().max(0) as u64
}

/// Checks a TOTP code (no replays) or an unused recovery code.
pub async fn verify_second_factor(state: &AppState, user_id: Uuid, code: &str) -> AppResult<bool> {
    let db = &state.db;
    let t = account::totp_state(db, user_id).await?;
    let (Some(secret), Some(_)) = (t.secret, t.enabled_at) else {
        return Ok(false);
    };
    let secret = state
        .cipher
        .decrypt(&secret, &totp_aad(user_id))
        .map_err(|e| AppError::internal(format!("decrypt TOTP secret: {e}")))?;
    let digits: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() == totp::DIGITS as usize && digits.bytes().all(|b| b.is_ascii_digit()) {
        let last = t.last_step.map(|s| s as u64);
        return match totp::verify(&secret, &digits, unix_now(), last) {
            Some(step) => Ok(account::use_totp_step(db, user_id, step as i64).await?),
            None => Ok(false),
        };
    }
    let used = account::use_recovery_code(db, user_id, &recovery_hash(code)).await?;
    if used {
        audit::record(
            db,
            AuditEvent::new(Some(user_id), "auth.recovery_code_used"),
        )
        .await;
    }
    Ok(used)
}

#[derive(Debug, Serialize)]
pub struct TotpEnrollment {
    pub secret: String,
    pub otpauth_uri: String,
    /// QR code of the URI, as an SVG document.
    pub qr_svg: String,
}

#[derive(Debug, Serialize)]
pub struct RecoveryCodes {
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SecurityStatus {
    pub two_factor_enabled: bool,
    pub recovery_codes_left: i64,
    pub password_reset_available: bool,
}

pub struct AccountService<'a> {
    state: &'a AppState,
}

impl<'a> AccountService<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    async fn check_password(&self, user_id: Uuid, password: Zeroizing<String>) -> AppResult<()> {
        let user = users::find_by_id(&self.state.db, user_id)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !verify_password(password, user.password_hash).await? {
            return Err(AppError::validation(
                "password",
                "the password is not correct",
            ));
        }
        Ok(())
    }

    pub async fn status(&self, user_id: Uuid) -> AppResult<SecurityStatus> {
        let t = account::totp_state(&self.state.db, user_id).await?;
        Ok(SecurityStatus {
            two_factor_enabled: t.enabled_at.is_some(),
            recovery_codes_left: if t.enabled_at.is_some() {
                account::recovery_codes_left(&self.state.db, user_id).await?
            } else {
                0
            },
            password_reset_available: self.state.mailer.is_enabled(),
        })
    }

    /// Changes the password and signs out every other session.
    pub async fn change_password(
        &self,
        user: &AuthUser,
        current: Zeroizing<String>,
        new: Zeroizing<String>,
    ) -> AppResult<u64> {
        let db = &self.state.db;
        let record = users::find_by_id(db, user.user_id)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !verify_password(current, record.password_hash).await? {
            audit::record(
                db,
                AuditEvent::new(Some(user.user_id), "auth.password_change_failed"),
            )
            .await;
            return Err(AppError::validation(
                "current_password",
                "the current password is not correct",
            ));
        }
        let hash = hash_password(new).await?;
        users::update_password_hash(db, user.user_id, &hash).await?;
        let revoked = account::delete_other_sessions(db, user.user_id, user.session_id).await?;
        audit::record(
            db,
            AuditEvent::new(Some(user.user_id), "auth.password_changed")
                .detail(json!({ "other_sessions_revoked": revoked })),
        )
        .await;
        Ok(revoked)
    }

    pub async fn sessions(&self, user: &AuthUser) -> AppResult<Vec<SessionInfo>> {
        let mut list = account::list_sessions(&self.state.db, user.user_id).await?;
        for s in &mut list {
            s.current = s.id == user.session_id;
        }
        Ok(list)
    }

    pub async fn revoke_session(&self, user: &AuthUser, id: Uuid) -> AppResult<()> {
        if !account::delete_session(&self.state.db, user.user_id, id).await? {
            return Err(AppError::NotFound("session"));
        }
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user.user_id), "auth.session_revoked").target("session", id),
        )
        .await;
        Ok(())
    }

    pub async fn revoke_other_sessions(&self, user: &AuthUser) -> AppResult<u64> {
        let n =
            account::delete_other_sessions(&self.state.db, user.user_id, user.session_id).await?;
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user.user_id), "auth.sessions_revoked")
                .detail(json!({ "count": n })),
        )
        .await;
        Ok(n)
    }

    /// Starts enrolment: a new secret that becomes active once a code from
    /// it is verified.
    pub async fn totp_enroll(&self, user: &AuthUser) -> AppResult<TotpEnrollment> {
        let mut secret = Zeroizing::new([0u8; 20]);
        OsRng.fill_bytes(secret.as_mut());
        let encrypted = self
            .state
            .cipher
            .encrypt(secret.as_ref(), &totp_aad(user.user_id))
            .map_err(|e| AppError::internal(format!("encrypt TOTP secret: {e}")))?;
        if !account::set_pending_totp(&self.state.db, user.user_id, &encrypted).await? {
            return Err(AppError::Conflict(
                "two-factor authentication is already enabled".into(),
            ));
        }
        let secret_b32 = totp::base32_encode(secret.as_ref());
        let uri = totp::provisioning_uri(&secret_b32, &user.email, ISSUER);
        let qr_svg = qrcode::QrCode::new(uri.as_bytes())
            .map_err(|e| AppError::internal(format!("QR code: {e}")))?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .build();
        Ok(TotpEnrollment {
            secret: secret_b32,
            otpauth_uri: uri,
            qr_svg,
        })
    }

    /// Verifies the first code and turns 2FA on; returns the recovery codes
    /// (shown once).
    pub async fn totp_confirm(&self, user: &AuthUser, code: &str) -> AppResult<RecoveryCodes> {
        let db = &self.state.db;
        let t = account::totp_state(db, user.user_id).await?;
        if t.enabled_at.is_some() {
            return Err(AppError::Conflict(
                "two-factor authentication is already enabled".into(),
            ));
        }
        let encrypted = t
            .secret
            .ok_or_else(|| AppError::Conflict("start enrolment before confirming a code".into()))?;
        let secret = self
            .state
            .cipher
            .decrypt(&encrypted, &totp_aad(user.user_id))
            .map_err(|e| AppError::internal(format!("decrypt TOTP secret: {e}")))?;
        let step = totp::verify(&secret, code, unix_now(), None)
            .ok_or_else(|| AppError::validation("code", "the code is not valid"))?;
        account::use_totp_step(db, user.user_id, step as i64).await?;
        let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
        let hashes: Vec<Vec<u8>> = codes.iter().map(|c| recovery_hash(c)).collect();
        account::enable_totp(db, user.user_id, &hashes).await?;
        audit::record(db, AuditEvent::new(Some(user.user_id), "auth.2fa_enabled")).await;
        Ok(RecoveryCodes {
            recovery_codes: codes,
        })
    }

    /// New recovery codes (a current second factor is required).
    pub async fn regenerate_recovery_codes(
        &self,
        user: &AuthUser,
        code: &str,
    ) -> AppResult<RecoveryCodes> {
        if !verify_second_factor(self.state, user.user_id, code).await? {
            return Err(AppError::validation("code", "the code is not valid"));
        }
        let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
        let hashes: Vec<Vec<u8>> = codes.iter().map(|c| recovery_hash(c)).collect();
        account::enable_totp(&self.state.db, user.user_id, &hashes).await?;
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user.user_id), "auth.recovery_codes_regenerated"),
        )
        .await;
        Ok(RecoveryCodes {
            recovery_codes: codes,
        })
    }

    pub async fn totp_disable(
        &self,
        user: &AuthUser,
        password: Zeroizing<String>,
        code: &str,
    ) -> AppResult<()> {
        self.check_password(user.user_id, password).await?;
        if !verify_second_factor(self.state, user.user_id, code).await? {
            return Err(AppError::validation("code", "the code is not valid"));
        }
        account::disable_totp(&self.state.db, user.user_id).await?;
        audit::record(
            &self.state.db,
            AuditEvent::new(Some(user.user_id), "auth.2fa_disabled"),
        )
        .await;
        Ok(())
    }

    /// Emails a reset link when the account exists. The response never
    /// reveals whether it does.
    pub async fn request_reset(&self, email: &str) -> AppResult<()> {
        if !self.state.mailer.is_enabled() {
            return Err(AppError::BadRequest(
                "password reset by email is not available on this server".into(),
            ));
        }
        let db = &self.state.db;
        let Some(user) = users::find_by_email(db, email.trim()).await? else {
            // Same work as the found path, so timing does not reveal accounts.
            let _ = verify_password(Zeroizing::new(String::new()), dummy_hash().to_string()).await;
            return Ok(());
        };
        if account::recent_resets(db, user.id).await? >= RESETS_PER_HOUR {
            tracing::info!(user_id = %user.id, "password reset rate limited");
            return Ok(());
        }
        let token = generate_token();
        account::insert_reset(
            db,
            user.id,
            &hash_token(&token),
            Utc::now() + ChronoDuration::minutes(RESET_TTL_MINUTES),
        )
        .await?;
        let link = format!(
            "{}/reset-password?token={}",
            self.state.config.app_base_url,
            token.as_str()
        );
        let text = format!(
            "Someone asked to reset the password of your QueryLab account ({}).\n\n\
             Open this link within {RESET_TTL_MINUTES} minutes to choose a new password:\n{link}\n\n\
             If this was not you, ignore this email; your password stays the same.",
            user.email
        );
        // Delivery problems are logged, not returned, so the response is the
        // same whether or not the account exists.
        if let Err(err) = self
            .state
            .mailer
            .send(Email {
                to: &user.email,
                subject: "Reset your QueryLab password",
                text,
            })
            .await
        {
            tracing::error!(user_id = %user.id, error = %err, "password reset email failed");
        }
        audit::record(
            db,
            AuditEvent::new(Some(user.id), "auth.password_reset_requested"),
        )
        .await;
        Ok(())
    }

    pub async fn reset_token_is_valid(&self, token: &str) -> AppResult<bool> {
        Ok(account::reset_is_live(&self.state.db, &hash_token(token)).await?)
    }

    /// Sets a new password from a reset link and signs out every session.
    pub async fn confirm_reset(&self, token: &str, new: Zeroizing<String>) -> AppResult<()> {
        let db = &self.state.db;
        let hash = hash_password(new).await?;
        let user_id = account::consume_reset(db, &hash_token(token))
            .await?
            .ok_or_else(|| {
                AppError::validation("token", "this reset link is invalid or has expired")
            })?;
        users::update_password_hash(db, user_id, &hash).await?;
        let revoked = sessions::delete_all_for_user(db, user_id).await?;
        audit::record(
            db,
            AuditEvent::new(Some(user_id), "auth.password_reset")
                .detail(json!({ "sessions_revoked": revoked })),
        )
        .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_codes_are_normalised() {
        let code = new_recovery_code();
        assert_eq!(code.len(), 11);
        assert_eq!(&code[5..6], "-");
        assert_eq!(
            recovery_hash(&code),
            recovery_hash(&code.to_uppercase().replace('-', " "))
        );
        assert_ne!(new_recovery_code(), new_recovery_code());
    }
}

use serde::Deserialize;
use zeroize::Zeroizing;

use crate::{
    errors::AppError,
    utils::validation::{Validate, Validator},
};

pub const MIN_PASSWORD_LEN: usize = 10;
pub const MAX_PASSWORD_LEN: usize = 128;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: Zeroizing<String>,
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: Zeroizing<String>,
}

/// Pragmatic email check: one `@`, non-empty local part and a dotted domain,
/// no whitespace. Deliverability is not verified.
pub fn is_plausible_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl Validate for RegisterRequest {
    fn validate(&self) -> Result<(), AppError> {
        let mut v = Validator::new();
        v.length("email", self.email.trim(), 3, 254).check(
            is_plausible_email(self.email.trim()),
            "email",
            "must be a valid email address",
        );
        let pw_len = self.password.chars().count();
        v.check(
            pw_len >= MIN_PASSWORD_LEN,
            "password",
            &format!("must be at least {MIN_PASSWORD_LEN} characters"),
        )
        .check(
            pw_len <= MAX_PASSWORD_LEN,
            "password",
            &format!("must be at most {MAX_PASSWORD_LEN} characters"),
        );
        if let Some(name) = &self.display_name {
            v.length("display_name", name.trim(), 0, 100)
                .no_control_chars("display_name", name);
        }
        v.finish()
    }
}

impl Validate for LoginRequest {
    fn validate(&self) -> Result<(), AppError> {
        Validator::new()
            .length("email", self.email.trim(), 1, 254)
            .check(!self.password.is_empty(), "password", "is required")
            .check(
                self.password.chars().count() <= MAX_PASSWORD_LEN,
                "password",
                "is too long",
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_checks() {
        for ok in ["a@b.co", "first.last+tag@example.com"] {
            assert!(is_plausible_email(ok), "{ok}");
        }
        for bad in [
            "",
            "a",
            "a@b",
            "@b.com",
            "a@.com",
            "a@b.com.",
            "a b@c.com",
            "a@b@c.com",
        ] {
            assert!(!is_plausible_email(bad), "{bad}");
        }
    }

    #[test]
    fn register_password_policy() {
        let short: RegisterRequest =
            serde_json::from_value(serde_json::json!({"email": "a@b.co", "password": "short"}))
                .unwrap();
        assert!(short.validate().is_err());
        let ok: RegisterRequest = serde_json::from_value(
            serde_json::json!({"email": "a@b.co", "password": "long-enough-pass"}),
        )
        .unwrap();
        assert!(ok.validate().is_ok());
    }
}

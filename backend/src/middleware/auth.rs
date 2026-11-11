//! Authentication extractor and session cookie helpers.

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderMap, HeaderValue},
};
use chrono::{DateTime, Utc};

use crate::{
    errors::AppError,
    services::auth::{AuthService, AuthUser},
    state::AppState,
};

/// Cookie name. In secure deployments the `__Host-` prefix forces
/// `Secure`, `Path=/` and no `Domain`, preventing subdomain cookie injection.
pub fn cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-ql_session"
    } else {
        "ql_session"
    }
}

pub fn session_cookie(token: &str, expires_at: DateTime<Utc>, secure: bool) -> HeaderValue {
    let max_age = (expires_at - Utc::now()).num_seconds().max(0);
    let mut cookie = format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}",
        cookie_name(secure)
    );
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).expect("token is base64url; cookie is valid ASCII")
}

pub fn clear_session_cookie(secure: bool) -> HeaderValue {
    let mut cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        cookie_name(secure)
    );
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).expect("static cookie is valid")
}

/// Extracts the session token from `Authorization: Bearer` or the cookie.
pub fn extract_token(headers: &HeaderMap, secure: bool) -> Option<String> {
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            return Some(token.trim().to_string());
        }
    }
    let name = cookie_name(secure);
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<AuthUser>() {
            return Ok(user.clone());
        }
        let token = extract_token(&parts.headers, state.config.cookie_secure)
            .ok_or(AppError::Unauthorized)?;
        let user = AuthService::new(state)
            .authenticate(&token)
            .await?
            .ok_or(AppError::Unauthorized)?;
        parts.extensions.insert(user.clone());
        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bearer_then_cookie() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            "a=1; ql_session=cookie-token; b=2".parse().unwrap(),
        );
        assert_eq!(extract_token(&h, false).as_deref(), Some("cookie-token"));
        // Secure mode only accepts the __Host- cookie.
        assert_eq!(extract_token(&h, true), None);
        h.insert(
            header::AUTHORIZATION,
            "Bearer bearer-token".parse().unwrap(),
        );
        assert_eq!(extract_token(&h, false).as_deref(), Some("bearer-token"));
    }

    #[test]
    fn cookie_attributes() {
        let c = session_cookie("tok", Utc::now() + chrono::Duration::hours(1), true);
        let s = c.to_str().unwrap();
        assert!(s.starts_with("__Host-ql_session=tok;"));
        for attr in ["HttpOnly", "SameSite=Lax", "Path=/", "Secure"] {
            assert!(s.contains(attr), "{attr}");
        }
        let dev = session_cookie("tok", Utc::now(), false);
        assert!(!dev.to_str().unwrap().contains("Secure"));
        assert!(clear_session_cookie(false)
            .to_str()
            .unwrap()
            .contains("Max-Age=0"));
    }
}

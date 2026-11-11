//! CSRF protection for cookie-authenticated requests.
//!
//! State-changing requests must carry `X-QueryLab-CSRF`. Browsers cannot add
//! custom headers to cross-site requests without a CORS preflight, and the
//! preflight only succeeds for configured origins. Combined with the
//! `SameSite=Lax` session cookie this blocks cross-site request forgery.

use axum::{
    extract::Request,
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::errors::AppError;

pub const CSRF_HEADER: &str = "x-querylab-csrf";

pub async fn require_csrf_header(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && req.uri().path().starts_with("/api/") && !req.headers().contains_key(CSRF_HEADER) {
        return AppError::Forbidden("missing CSRF header".into()).into_response();
    }
    next.run(req).await
}

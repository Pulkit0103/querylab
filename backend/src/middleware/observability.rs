//! Response hardening, request logging and panic capture.
//!
//! Every response carries defensive security headers. Each request produces
//! one structured log line (method, path, status, latency, request and user
//! id) at `info`, or at `error` for 5xx responses, which is what log-based
//! alerting keys on. Panics in handlers become a JSON 500 instead of a
//! dropped connection, and are logged with the panic message.

use std::{any::Any, time::Duration};

use axum::{
    http::{header, HeaderMap, HeaderName, HeaderValue, Request, Response},
    response::IntoResponse,
};
use tower_http::trace::{MakeSpan, OnResponse};
use tracing::Span;

use crate::errors::AppError;

pub const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Adds security headers unless a handler already set them. The API only
/// serves JSON, so the CSP forbids everything.
pub fn apply_security_headers(headers: &mut HeaderMap, hsts: bool) {
    let defaults: [(HeaderName, &'static str); 5] = [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "no-referrer"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; frame-ancestors 'none'",
        ),
        // Responses contain user data; never let shared caches keep them.
        (header::CACHE_CONTROL, "no-store"),
    ];
    for (name, value) in defaults {
        headers
            .entry(name)
            .or_insert(HeaderValue::from_static(value));
    }
    if hsts {
        headers
            .entry(header::STRICT_TRANSPORT_SECURITY)
            .or_insert(HeaderValue::from_static(
                "max-age=31536000; includeSubDomains",
            ));
    }
}

/// Turns a handler panic into the standard JSON 500 envelope.
pub fn panic_response(panic: Box<dyn Any + Send + 'static>) -> axum::response::Response {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload");
    // `AppError::Internal` logs the context at error level.
    AppError::internal(format!("handler panicked: {message}")).into_response()
}

/// Request span: method, path (never the query string, which may carry
/// search terms), request id, and a `user_id` slot filled in by the auth
/// extractor.
#[derive(Clone, Copy, Default)]
pub struct RequestSpan;

impl<B> MakeSpan<B> for RequestSpan {
    fn make_span(&mut self, req: &Request<B>) -> Span {
        let request_id = req
            .headers()
            .get(&REQUEST_ID)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-");
        tracing::info_span!(
            "http",
            method = %req.method(),
            path = %req.uri().path(),
            request_id = %request_id,
            user_id = tracing::field::Empty,
        )
    }
}

/// One log line per response; server errors are logged at `error`.
#[derive(Clone, Copy, Default)]
pub struct LogResponse;

impl<B> OnResponse<B> for LogResponse {
    fn on_response(self, res: &Response<B>, latency: Duration, _span: &Span) {
        let status = res.status().as_u16();
        let latency_ms = latency.as_millis() as u64;
        if res.status().is_server_error() {
            tracing::error!(status, latency_ms, "request failed");
        } else {
            tracing::info!(status, latency_ms, "request completed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_headers_do_not_override_handler_values() {
        let mut h = HeaderMap::new();
        h.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("max-age=60"),
        );
        apply_security_headers(&mut h, false);
        assert_eq!(h[header::CACHE_CONTROL], "max-age=60");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(h.get(header::STRICT_TRANSPORT_SECURITY).is_none());
        apply_security_headers(&mut h, true);
        assert!(h.get(header::STRICT_TRANSPORT_SECURITY).is_some());
    }

    #[test]
    fn panics_become_generic_json_500s() {
        let res = panic_response(Box::new("boom: secret detail"));
        assert_eq!(res.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let res = panic_response(Box::new(String::from("owned")));
        assert_eq!(res.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}

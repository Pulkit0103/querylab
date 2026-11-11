//! HTTP handlers: extract and validate request data, delegate to services and
//! map results to HTTP responses. No business logic lives here.

pub mod auth;
pub mod connections;
pub mod health;
pub mod version;

use crate::errors::AppError;

/// JSON 404 for unknown routes, matching the API error envelope.
pub async fn not_found() -> AppError {
    AppError::NotFound("route")
}

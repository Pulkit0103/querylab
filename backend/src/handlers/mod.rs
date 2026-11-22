//! HTTP handlers: extract and validate request data, delegate to services and
//! map results to HTTP responses. No business logic lives here.

pub mod account;
pub mod audit;
pub mod auth;
pub mod connections;
pub mod data;
pub mod data_jobs;
pub mod diagrams;
pub mod health;
pub mod library;
pub mod migrations;
pub mod monitoring;
pub mod performance;
pub mod query;
pub mod schema;
pub mod version;
pub mod workspace;
pub mod workspaces;

use crate::errors::AppError;

/// JSON 404 for unknown routes, matching the API error envelope.
pub async fn not_found() -> AppError {
    AppError::NotFound("route")
}

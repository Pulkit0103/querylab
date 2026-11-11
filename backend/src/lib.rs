//! QueryLab API library crate. `main.rs` is a thin binary wrapper so the full
//! router can be constructed in integration tests.
//!
//! Layering: `routes` → `handlers` (HTTP) → `services` (business logic) →
//! `database` (repositories, SQL). `models` holds records and DTOs.

pub mod config;
pub mod database;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod state;
pub mod telemetry;
pub mod utils;

pub use routes::build_router;
pub use state::AppState;

//! QueryLab API library crate. `main.rs` is a thin binary wrapper so the full
//! router can be constructed in integration tests.

pub mod config;
pub mod database;
pub mod handlers;
pub mod routes;
pub mod state;
pub mod telemetry;

pub use routes::build_router;
pub use state::AppState;

//! Business logic. Services are constructed per request from [`AppState`]
//! (cheap borrows) and are independent of HTTP types.
//!
//! [`AppState`]: crate::state::AppState

pub mod auth;
pub mod connections;
pub mod schema;
pub mod target_db;

//! Business logic. Services are constructed per request from [`AppState`]
//! (cheap borrows) and are independent of HTTP types.
//!
//! [`AppState`]: crate::state::AppState

pub mod auth;
pub mod connections;
pub mod data_studio;
pub mod ddl;
pub mod diagrams;
pub mod execution;
pub mod library;
pub mod monitoring;
pub mod performance;
pub mod schema;
pub mod sql_builder;
pub mod target_db;
pub mod value_decode;
pub mod workspace;

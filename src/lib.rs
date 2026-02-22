//! Public library interface for `claude-code-rusty-proxy`.
//!
//! Re-exports all modules so that integration tests and external consumers
//! can construct the server programmatically (e.g. via [`server::build_router`]).

pub mod adapter;
pub mod cli;
pub mod config;
pub mod error;
pub mod handlers;
pub mod server;
pub mod session;
pub mod types;

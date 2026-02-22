//! HTTP request handlers for all public endpoints.
//!
//! * [`health`] – `GET /health`
//! * [`models`] – `GET /v1/models`
//! * [`chat`]   – `POST /v1/chat/completions`

pub mod chat;
pub mod health;
pub mod models;

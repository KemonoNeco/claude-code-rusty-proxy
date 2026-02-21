//! Health check endpoint.

use axum::Json;
use serde_json::{json, Value};

/// GET /health
pub async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "provider": "claude-code-rusty-proxy"
    }))
}

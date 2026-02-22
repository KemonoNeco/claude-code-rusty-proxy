//! Unified error type that maps proxy failures to OpenAI-shaped JSON responses.
//!
//! Every variant carries enough context to produce both a human-readable
//! message and the correct HTTP status code. The [`IntoResponse`] impl
//! serialises the error into the standard OpenAI error envelope:
//!
//! ```json
//! { "error": { "message": "...", "type": "...", "param": null, "code": null } }
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Proxy error types, each mapping to an appropriate HTTP status and OpenAI error shape.
#[derive(Debug)]
pub enum ProxyError {
    /// Claude CLI binary wasn't found on the system.
    CliNotFound(String),
    /// Failed to spawn the CLI process.
    CliSpawnFailed(String),
    /// CLI process timed out.
    CliTimeout(u64),
    /// CLI exited with a non-zero status.
    CliExitError { code: i32, stderr: String },
    /// Invalid request from the client.
    InvalidRequest(String),
    /// Internal server error.
    Internal(String),
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CliNotFound(msg) => write!(f, "CLI not found: {}", msg),
            Self::CliSpawnFailed(msg) => write!(f, "CLI spawn failed: {}", msg),
            Self::CliTimeout(secs) => write!(f, "CLI timed out after {}s", secs),
            Self::CliExitError { code, stderr } => {
                write!(f, "CLI exited with code {}: {}", code, stderr)
            }
            Self::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            ProxyError::CliNotFound(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "server_error", msg.clone())
            }
            ProxyError::CliSpawnFailed(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                msg.clone(),
            ),
            ProxyError::CliTimeout(secs) => (
                StatusCode::GATEWAY_TIMEOUT,
                "timeout",
                format!("Request timed out after {}s", secs),
            ),
            ProxyError::CliExitError { code, stderr } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                format!("CLI exited with code {}: {}", code, stderr),
            ),
            ProxyError::InvalidRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                msg.clone(),
            ),
            ProxyError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                msg.clone(),
            ),
        };

        let body = json!({
            "error": {
                "message": message,
                "type": error_type,
                "param": null,
                "code": null
            }
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    //! Verify each `ProxyError` variant produces the correct HTTP status and
    //! OpenAI-shaped error JSON (`error.message`, `error.type`, `error.param`,
    //! `error.code`).

    use super::*;
    use axum::body::to_bytes;

    /// Helper: convert a `ProxyError` into its HTTP status + JSON body.
    async fn error_to_parts(error: ProxyError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    /// `CliNotFound` -> 503 Service Unavailable.
    #[tokio::test]
    async fn test_cli_not_found() {
        let (status, json) = error_to_parts(ProxyError::CliNotFound("not found".into())).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json["error"]["type"], "server_error");
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    /// `CliSpawnFailed` -> 500 Internal Server Error.
    #[tokio::test]
    async fn test_cli_spawn_failed() {
        let (status, json) =
            error_to_parts(ProxyError::CliSpawnFailed("permission denied".into())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["error"]["type"], "server_error");
    }

    /// `CliTimeout` -> 504 Gateway Timeout.
    #[tokio::test]
    async fn test_cli_timeout() {
        let (status, json) = error_to_parts(ProxyError::CliTimeout(300)).await;
        assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(json["error"]["type"], "timeout");
        assert!(json["error"]["message"].as_str().unwrap().contains("300"));
    }

    /// `CliExitError` -> 500 Internal Server Error with exit code in message.
    #[tokio::test]
    async fn test_cli_exit_error() {
        let (status, json) = error_to_parts(ProxyError::CliExitError {
            code: 1,
            stderr: "auth failed".into(),
        })
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["error"]["type"], "server_error");
    }

    /// `InvalidRequest` -> 400 Bad Request with `invalid_request_error` type.
    #[tokio::test]
    async fn test_invalid_request() {
        let (status, json) =
            error_to_parts(ProxyError::InvalidRequest("missing messages".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error"]["type"], "invalid_request_error");
    }

    /// `Internal` -> 500 Internal Server Error.
    #[tokio::test]
    async fn test_internal_error() {
        let (status, json) = error_to_parts(ProxyError::Internal("unexpected".into())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["error"]["type"], "server_error");
    }

    /// The error envelope must always contain `message`, `type`, `param`, `code`.
    #[tokio::test]
    async fn test_error_json_shape() {
        let (_, json) = error_to_parts(ProxyError::InvalidRequest("test".into())).await;
        // OpenAI error format has error.message, error.type, error.param, error.code
        assert!(json["error"]["message"].is_string());
        assert!(json["error"]["type"].is_string());
        assert!(json["error"]["param"].is_null());
        assert!(json["error"]["code"].is_null());
    }
}

//! Integration tests for the HTTP endpoints.
//!
//! Uses `tower::ServiceExt::oneshot` to drive the Axum router in-process
//! without binding a real TCP socket. This keeps the tests fast and
//! deterministic — no `claude` binary is needed because only request
//! validation paths are exercised (not actual CLI invocations).
//!
//! ## Test inventory
//!
//! | Test | Validates |
//! |------|-----------|
//! | `test_health_endpoint` | `GET /health` returns 200 with `{"status":"ok"}`. |
//! | `test_models_endpoint` | `GET /v1/models` lists all 3 models in OpenAI format. |
//! | `test_chat_completions_empty_messages` | Empty messages array -> 400 `invalid_request_error`. |
//! | `test_chat_completions_invalid_json` | Malformed JSON body -> 400/422. |
//! | `test_chat_completions_system_only_messages` | System-only messages (no user content) -> 400. |

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use tower::util::ServiceExt;

use claude_code_rusty_proxy::config::Config;
use claude_code_rusty_proxy::server;

/// Build a test-only [`Config`] with port 0 and sensible defaults.
fn test_config() -> Config {
    Config {
        port: 0,
        host: "127.0.0.1".to_string(),
        timeout: 300,
        default_model: "sonnet".to_string(),
        verbose: false,
    }
}

/// Construct the full router with test state (no TCP listener).
fn build_app() -> axum::Router {
    let state = server::create_state(test_config());
    server::build_router(state)
}

/// `GET /health` should return 200 with `{"status":"ok","provider":"claude-code-rusty-proxy"}`.
#[tokio::test]
async fn test_health_endpoint() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["provider"], "claude-code-rusty-proxy");
}

/// `GET /v1/models` should list opus, sonnet, and haiku with correct metadata.
#[tokio::test]
async fn test_models_endpoint() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["object"], "list");
    let models = json["data"].as_array().unwrap();
    assert_eq!(models.len(), 3);

    let ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"claude-opus-4"));
    assert!(ids.contains(&"claude-sonnet-4"));
    assert!(ids.contains(&"claude-haiku-4"));

    for model in models {
        assert_eq!(model["object"], "model");
        assert_eq!(model["owned_by"], "anthropic");
    }
}

/// `POST /v1/chat/completions` with `messages: []` -> 400 `invalid_request_error`.
#[tokio::test]
async fn test_chat_completions_empty_messages() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"claude-sonnet-4","messages":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["type"], "invalid_request_error");
    assert!(json["error"]["message"].as_str().unwrap().contains("empty"));
}

/// Assert the response body matches the OpenAI error JSON structure:
/// `{ "error": { "message": "…", "type": "…", "param": null, "code": null } }`.
fn assert_openai_error(json: &serde_json::Value, expected_type: &str) {
    assert!(json["error"].is_object(), "error field must be an object");
    assert!(
        json["error"]["message"].is_string(),
        "error.message must be a string"
    );
    assert_eq!(
        json["error"]["type"].as_str().unwrap(),
        expected_type,
        "error.type mismatch"
    );
    assert!(json["error"]["param"].is_null(), "error.param must be null");
    assert!(json["error"]["code"].is_null(), "error.code must be null");
}

/// Non-JSON body -> Axum returns 400 or 422 (deserialization rejection).
#[tokio::test]
async fn test_chat_completions_invalid_json() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 422 for deserialization errors
    let status = response.status();
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY);

    // Axum's JSON rejection may not produce JSON, just verify the body is non-empty
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!body.is_empty());
}

/// A system-only message array has no user content -> 400 `invalid_request_error`.
#[tokio::test]
async fn test_chat_completions_system_only_messages() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"claude-sonnet-4","messages":[{"role":"system","content":"Just a system message"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // System-only messages should fail validation (no user content)
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_openai_error(&json, "invalid_request_error");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No user content"));
}

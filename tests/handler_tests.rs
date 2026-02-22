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
    assert!(ids.contains(&"claude-opus-4-6"));
    assert!(ids.contains(&"claude-sonnet-4-6"));
    assert!(ids.contains(&"claude-haiku-4-5"));

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
                .body(Body::from(r#"{"model":"claude-sonnet-4-6","messages":[]}"#))
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
                    r#"{"model":"claude-sonnet-4-6","messages":[{"role":"system","content":"Just a system message"}]}"#,
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

// ── Adversarial HTTP boundary tests ─────────────────────────────

/// Body exceeding the 10 MB limit must be rejected (413 Payload Too Large).
#[tokio::test]
async fn test_body_limit_rejected() {
    let app = build_app();
    // Build a body just over 10 MB
    let huge_content = "x".repeat(11 * 1024 * 1024);
    let body_json = format!(
        r#"{{"model":"sonnet","messages":[{{"role":"user","content":"{}"}}]}}"#,
        huge_content
    );
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(body_json))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// Missing Content-Type header on POST must be rejected (415 or 422).
#[tokio::test]
async fn test_missing_content_type() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .body(Body::from(
                    r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::UNSUPPORTED_MEDIA_TYPE || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 415 or 422, got {}",
        status
    );
}

/// Wrong Content-Type (text/plain) on POST must be rejected.
#[tokio::test]
async fn test_wrong_content_type() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "text/plain")
                .body(Body::from(
                    r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::UNSUPPORTED_MEDIA_TYPE || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 415 or 422, got {}",
        status
    );
}

/// Messages with only unknown roles (e.g. "developer") produce no user
/// content and must be rejected with 400.
#[tokio::test]
async fn test_messages_with_only_unknown_roles() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"sonnet","messages":[{"role":"developer","content":"secret stuff"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_openai_error(&json, "invalid_request_error");
}

/// Thread ID with path traversal characters must not cause filesystem access.
/// The session manager is in-memory so this just verifies no crash.
#[tokio::test]
async fn test_thread_id_path_traversal() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}],"thread_id":"../../../etc/passwd"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Path traversal in thread_id should be rejected as a bad request
    let status = response.status();
    assert!(
        status.is_client_error() || status.is_server_error(),
        "Expected error status, got {}",
        status
    );
}

/// Thread ID with null bytes must not crash.
#[tokio::test]
async fn test_thread_id_null_bytes() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}],"thread_id":"thread\u0000evil"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should not crash
    let status = response.status();
    assert!(
        status.is_client_error() || status.is_server_error(),
        "Expected error status, got {}",
        status
    );
}

/// Empty POST body must be rejected (400 or 422).
#[tokio::test]
async fn test_empty_body() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        status
    );
}

/// JSON array body instead of object must be rejected.
#[tokio::test]
async fn test_json_array_body() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(r#"[{"role":"user","content":"Hi"}]"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}",
        status
    );
}

/// GET on /v1/chat/completions must be rejected (405 Method Not Allowed).
#[tokio::test]
async fn test_wrong_method_on_chat() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// POST on /health must be rejected (405 Method Not Allowed).
#[tokio::test]
async fn test_post_to_health() {
    let app = build_app();
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// Deeply nested JSON content must not cause a stack overflow.
#[tokio::test]
async fn test_deeply_nested_json() {
    let app = build_app();
    // Build deeply nested content parts
    let mut nested = r#"{"type":"text","text":"deep"}"#.to_string();
    for _ in 0..100 {
        nested = format!(r#"{{"type":"text","text":"layer","nested":{}}}"#, nested);
    }
    let body = format!(
        r#"{{"model":"sonnet","messages":[{{"role":"user","content":[{}]}}]}}"#,
        nested
    );
    let response: Response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should not stack overflow; will get some response (error or success attempt)
    let status = response.status();
    assert!(
        status.is_client_error() || status.is_server_error() || status.is_success(),
        "Should get some HTTP response, got {}",
        status
    );
}

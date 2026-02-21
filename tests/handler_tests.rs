//! Integration tests for HTTP handlers using tower::ServiceExt.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use tower::util::ServiceExt;

use claude_code_rusty_proxy::config::Config;
use claude_code_rusty_proxy::server;

fn test_config() -> Config {
    Config {
        port: 0,
        host: "127.0.0.1".to_string(),
        timeout: 300,
        default_model: "sonnet".to_string(),
        verbose: false,
    }
}

fn build_app() -> axum::Router {
    let state = server::create_state(test_config());
    server::build_router(state)
}

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
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY
    );
}

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
}

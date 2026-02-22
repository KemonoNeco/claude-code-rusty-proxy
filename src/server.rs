//! HTTP server bootstrap: router construction, shared state, and graceful shutdown.
//!
//! The server uses [`axum`] with permissive CORS (for browser-based clients)
//! and a 10 MB request body limit. A background task periodically purges
//! expired session mappings from the [`SessionManager`].

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::handlers::chat::{self, AppState};
use crate::handlers::health;
use crate::handlers::models;
use crate::session::SessionManager;

/// Construct the Axum [`Router`] with all routes wired to the shared `state`.
///
/// Layers applied (outermost first):
/// * `TraceLayer` – request/response tracing via `tracing`
/// * `CorsLayer::permissive` – allow any origin
/// * `DefaultBodyLimit` – cap request bodies at 10 MB
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/v1/models", get(models::list_models))
        .route("/v1/chat/completions", post(chat::chat_completions))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Build the [`Arc<AppState>`] that is shared across all request handlers.
///
/// Session TTL is set to 10 × the configured request timeout so that
/// multi-turn conversations survive between requests.
pub fn create_state(config: Config) -> Arc<AppState> {
    let ttl = std::time::Duration::from_secs(config.timeout * 10);
    Arc::new(AppState {
        config,
        session_manager: SessionManager::new(ttl),
    })
}

/// Bind, serve, and block until a shutdown signal is received.
///
/// A background tokio task runs every 5 minutes to evict expired sessions.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", config.host, config.port);
    let state = create_state(config);

    // Periodic session cleanup (every 5 min).
    let cleanup_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            cleanup_state.session_manager.cleanup_expired();
        }
    });

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Block until either `SIGINT` (Ctrl-C) or `SIGTERM` is received.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, stopping server...");
}

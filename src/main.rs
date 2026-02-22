mod adapter;
mod cli;
mod config;
mod error;
mod handlers;
mod server;
mod session;
mod types;

use clap::Parser;
use config::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::parse();

    // Initialize tracing
    let filter = if config.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // Print banner
    println!();
    println!("  claude-code-rusty-proxy v{}", env!("CARGO_PKG_VERSION"));
    println!("  OpenAI-compatible API for Claude CLI");
    println!();

    // Verify CLI
    match cli::verify::verify_cli().await {
        Ok(version) => {
            tracing::info!("Claude CLI found: {}", version);
        }
        Err(e) => {
            tracing::warn!("Claude CLI verification failed: {} (continuing anyway)", e);
        }
    }

    println!("  Listening on https://{}:{}", config.host, config.port);
    println!("  Default model: {}", config.default_model);
    println!();
    println!("  Endpoints:");
    println!("    GET  /health");
    println!("    GET  /v1/models");
    println!("    POST /v1/chat/completions");
    println!();

    server::run(config).await
}

//! Server configuration parsed from CLI arguments and environment variables.
//!
//! Every field can be set via a `--long-flag` **or** the corresponding
//! `PROXY_*` environment variable. When both are set the CLI flag wins.

use clap::Parser;

/// Top-level configuration for the proxy server.
///
/// Parsed once at startup via [`clap::Parser`] and then shared (via
/// [`std::sync::Arc`]) with every handler through Axum's state layer.
#[derive(Parser, Debug, Clone)]
#[command(name = "claude-code-rusty-proxy", version, about)]
pub struct Config {
    /// TCP port the HTTP server will bind to.
    #[arg(long, default_value = "3456", env = "PROXY_PORT")]
    pub port: u16,

    /// Network address to listen on (`0.0.0.0` for all interfaces).
    #[arg(long, default_value = "127.0.0.1", env = "PROXY_HOST")]
    pub host: String,

    /// Per-request timeout in seconds for waiting on the Claude CLI.
    #[arg(long, default_value = "300", env = "PROXY_TIMEOUT")]
    pub timeout: u64,

    /// Fallback Claude model used when the request model is unrecognised.
    /// Accepts short aliases like `sonnet`, `opus`, `haiku`.
    #[arg(long, default_value = "sonnet", env = "PROXY_DEFAULT_MODEL")]
    pub default_model: String,

    /// Enable debug-level tracing output.
    #[arg(long, env = "PROXY_VERBOSE")]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    //! Verify CLI argument parsing defaults and custom overrides.

    use super::*;

    /// All defaults should match the `#[arg(default_value = …)]` annotations.
    #[test]
    fn test_default_values() {
        let config = Config::parse_from(["test"]);
        assert_eq!(config.port, 3456);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.timeout, 300);
        assert_eq!(config.default_model, "sonnet");
        assert!(!config.verbose);
    }

    /// Explicit flags override every default.
    #[test]
    fn test_custom_values() {
        let config = Config::parse_from([
            "test",
            "--port",
            "8080",
            "--host",
            "0.0.0.0",
            "--timeout",
            "600",
            "--default-model",
            "opus",
            "--verbose",
        ]);
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.timeout, 600);
        assert_eq!(config.default_model, "opus");
        assert!(config.verbose);
    }
}

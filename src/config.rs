use clap::Parser;

/// Claude Code Rusty Proxy - OpenAI-compatible API server wrapping the Claude CLI.
#[derive(Parser, Debug, Clone)]
#[command(name = "claude-code-rusty-proxy", version, about)]
pub struct Config {
    /// Port to listen on
    #[arg(long, default_value = "3456", env = "PROXY_PORT")]
    pub port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1", env = "PROXY_HOST")]
    pub host: String,

    /// Request timeout in seconds
    #[arg(long, default_value = "300", env = "PROXY_TIMEOUT")]
    pub timeout: u64,

    /// Default Claude model to use when not specified
    #[arg(long, default_value = "sonnet", env = "PROXY_DEFAULT_MODEL")]
    pub default_model: String,

    /// Enable verbose logging
    #[arg(long, env = "PROXY_VERBOSE")]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = Config::parse_from(["test"]);
        assert_eq!(config.port, 3456);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.timeout, 300);
        assert_eq!(config.default_model, "sonnet");
        assert!(!config.verbose);
    }

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

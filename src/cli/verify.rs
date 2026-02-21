//! CLI binary verification.

use crate::error::ProxyError;
use tokio::process::Command;

/// Verify the Claude CLI is installed and return its version string.
pub async fn verify_cli() -> Result<String, ProxyError> {
    let output = Command::new("claude")
        .arg("--version")
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProxyError::CliNotFound(
                    "Claude CLI binary not found. Install from https://docs.anthropic.com/en/docs/claude-code".to_string(),
                )
            } else {
                ProxyError::CliSpawnFailed(format!("Failed to run 'claude --version': {}", e))
            }
        })?;

    if !output.status.success() {
        return Err(ProxyError::CliExitError {
            code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(version)
}

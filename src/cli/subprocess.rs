//! Claude CLI subprocess management.
//!
//! Spawns the `claude` binary and parses its NDJSON streaming output.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::cli::types::{ClaudeStreamEvent, UsageInfo};
use crate::config::Config;
use crate::error::ProxyError;

/// Parsed output from a Claude CLI invocation.
#[derive(Debug, Default)]
pub struct CliOutput {
    /// Captured session ID from the `system` event.
    pub session_id: Option<String>,
    /// Aggregated text content from assistant events.
    pub text_content: String,
    /// Tool calls extracted from assistant events (id, name, input_json).
    pub tool_calls: Vec<CliToolCall>,
    /// Whether the result event indicated an error.
    pub is_error: bool,
    /// Aggregated input token count.
    pub input_tokens: u32,
    /// Aggregated output token count.
    pub output_tokens: u32,
    /// Result text from the `result` event (fallback content).
    pub result_text: Option<String>,
}

/// A tool call extracted from the CLI output.
#[derive(Debug, Clone)]
pub struct CliToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

/// Spawn the Claude CLI and collect its full output (non-streaming).
pub async fn run_claude(
    prompt: &str,
    system_prompt: Option<&str>,
    model: &str,
    session_id: Option<&str>,
    config: &Config,
) -> Result<CliOutput, ProxyError> {
    let mut cmd = Command::new("claude");
    cmd.arg("--print")
        .arg(prompt)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--model")
        .arg(model)
        .arg("--max-turns")
        .arg("1");

    if let Some(system) = system_prompt {
        cmd.arg("--system-prompt").arg(system);
    }

    if let Some(sid) = session_id {
        cmd.arg("--resume").arg(sid);
    }

    // Allow running inside Claude Code by removing CLAUDE_CODE env var
    cmd.env_remove("CLAUDE_CODE");

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ProxyError::CliNotFound("Claude CLI binary not found".to_string())
        } else {
            ProxyError::CliSpawnFailed(format!("Failed to spawn claude: {}", e))
        }
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProxyError::CliSpawnFailed("Failed to capture stdout".to_string()))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ProxyError::CliSpawnFailed("Failed to capture stderr".to_string()))?;

    // Spawn stderr reader for debug logging
    let stderr_handle = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let mut stderr_output = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "claude_cli", "stderr: {}", line);
            if !stderr_output.is_empty() {
                stderr_output.push('\n');
            }
            stderr_output.push_str(&line);
        }
        stderr_output
    });

    // Parse NDJSON stdout with timeout
    let timeout = Duration::from_secs(config.timeout);
    let parse_result = tokio::time::timeout(timeout, parse_ndjson_stream(stdout)).await;

    let output = match parse_result {
        Ok(result) => result?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(ProxyError::CliTimeout(config.timeout));
        }
    };

    // Wait for process exit
    let status = child
        .wait()
        .await
        .map_err(|e| ProxyError::Internal(format!("Failed waiting for claude process: {}", e)))?;

    let stderr_output = stderr_handle.await.unwrap_or_default();

    if !status.success() && !output.is_error {
        let code = status.code().unwrap_or(-1);
        return Err(ProxyError::CliExitError {
            code,
            stderr: if stderr_output.is_empty() {
                format!("Process exited with code {}", code)
            } else {
                stderr_output
            },
        });
    }

    Ok(output)
}

/// Spawn the Claude CLI and return the child process + a stream of events (for SSE streaming).
pub async fn spawn_claude_streaming(
    prompt: &str,
    system_prompt: Option<&str>,
    model: &str,
    session_id: Option<&str>,
) -> Result<(tokio::process::Child, tokio::process::ChildStdout), ProxyError> {
    let mut cmd = Command::new("claude");
    cmd.arg("--print")
        .arg(prompt)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--model")
        .arg(model)
        .arg("--max-turns")
        .arg("1");

    if let Some(system) = system_prompt {
        cmd.arg("--system-prompt").arg(system);
    }

    if let Some(sid) = session_id {
        cmd.arg("--resume").arg(sid);
    }

    cmd.env_remove("CLAUDE_CODE");

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ProxyError::CliNotFound("Claude CLI binary not found".to_string())
        } else {
            ProxyError::CliSpawnFailed(format!("Failed to spawn claude: {}", e))
        }
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProxyError::CliSpawnFailed("Failed to capture stdout".to_string()))?;

    // Spawn stderr reader for debug logging
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "claude_cli", "stderr: {}", line);
            }
        });
    }

    Ok((child, stdout))
}

/// Parse NDJSON streaming output from the Claude CLI into a collected output.
pub async fn parse_ndjson_stream(
    stdout: tokio::process::ChildStdout,
) -> Result<CliOutput, ProxyError> {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut output = CliOutput::default();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let event: ClaudeStreamEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => {
                tracing::trace!(target: "claude_cli", "Skipping non-JSON line: {}", truncate_str(&line, 100));
                continue;
            }
        };

        process_event(&mut output, &event);
    }

    Ok(output)
}

/// Parse a single NDJSON line into a `ClaudeStreamEvent`.
pub fn parse_event(line: &str) -> Option<ClaudeStreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// Process a single event, updating the accumulated output.
pub fn process_event(output: &mut CliOutput, event: &ClaudeStreamEvent) {
    aggregate_usage(output, event);

    match event.event_type.as_str() {
        "system" => {
            if let Some(ref sid) = event.session_id {
                output.session_id = Some(sid.clone());
                tracing::debug!(session_id = %sid, "Captured CLI session ID");
            }
        }
        "assistant" => {
            if let Some(ref msg) = event.message {
                if let Some(ref blocks) = msg.content {
                    for block in blocks {
                        match block.block_type.as_str() {
                            "text" => {
                                if let Some(ref text) = block.text {
                                    if !text.is_empty() {
                                        if !output.text_content.is_empty() {
                                            output.text_content.push('\n');
                                        }
                                        output.text_content.push_str(text);
                                    }
                                }
                            }
                            "tool_use" => {
                                if let (Some(id), Some(name)) = (&block.id, &block.name) {
                                    let args = block
                                        .input
                                        .as_ref()
                                        .map(|v| serde_json::to_string(v).unwrap_or_default())
                                        .unwrap_or_else(|| "{}".to_string());
                                    output.tool_calls.push(CliToolCall {
                                        id: id.clone(),
                                        name: name.clone(),
                                        arguments_json: args,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        "result" => {
            output.is_error = event.is_error.unwrap_or(false);
            if let Some(ref result) = event.result {
                if let Some(text) = result.as_str() {
                    if !text.is_empty() {
                        output.result_text = Some(text.to_string());
                    }
                }
            }
        }
        _ => {}
    }
}

/// Aggregate usage information from an event.
fn aggregate_usage(output: &mut CliOutput, event: &ClaudeStreamEvent) {
    if let Some(ref usage) = event.usage {
        add_usage(output, usage);
    }
    if let Some(ref msg) = event.message {
        if let Some(ref usage) = msg.usage {
            add_usage(output, usage);
        }
    }
}

fn add_usage(output: &mut CliOutput, usage: &UsageInfo) {
    output.input_tokens = output.input_tokens.saturating_add(usage.input_tokens);
    output.output_tokens = output.output_tokens.saturating_add(usage.output_tokens);
}

fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ndjson_from_str(ndjson: &str) -> CliOutput {
        let mut output = CliOutput::default();
        for line in ndjson.lines() {
            if let Some(event) = parse_event(line) {
                process_event(&mut output, &event);
            }
        }
        output
    }

    #[test]
    fn test_parse_system_event_captures_session_id() {
        let ndjson = r#"{"type":"system","session_id":"sess-abc-123","subtype":"init"}
{"type":"result","is_error":false,"result":"Done"}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.session_id.as_deref(), Some("sess-abc-123"));
    }

    #[test]
    fn test_parse_assistant_text_aggregated() {
        let ndjson = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello "}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"world!"}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.text_content, "Hello \nworld!");
    }

    #[test]
    fn test_parse_tool_use_blocks() {
        let ndjson = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"ls"}}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.tool_calls.len(), 1);
        assert_eq!(output.tool_calls[0].id, "toolu_01");
        assert_eq!(output.tool_calls[0].name, "Bash");
        assert!(output.tool_calls[0].arguments_json.contains("command"));
    }

    #[test]
    fn test_parse_mixed_text_and_tool_use() {
        let ndjson = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Let me check."},{"type":"tool_use","id":"t1","name":"Read","input":{"path":"file.txt"}}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.text_content, "Let me check.");
        assert_eq!(output.tool_calls.len(), 1);
        assert_eq!(output.tool_calls[0].name, "Read");
    }

    #[test]
    fn test_parse_result_success() {
        let ndjson = r#"{"type":"result","is_error":false,"result":"All done.","num_turns":3}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert!(!output.is_error);
        assert_eq!(output.result_text.as_deref(), Some("All done."));
    }

    #[test]
    fn test_parse_result_error() {
        let ndjson = r#"{"type":"result","is_error":true,"result":"Something went wrong"}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert!(output.is_error);
    }

    #[test]
    fn test_parse_usage_info_aggregated() {
        let ndjson = r#"{"type":"assistant","usage":{"input_tokens":100,"output_tokens":50},"message":{"content":[{"type":"text","text":"Hi"}]}}
{"type":"assistant","usage":{"input_tokens":0,"output_tokens":30},"message":{"content":[{"type":"text","text":" there"}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.input_tokens, 100);
        assert_eq!(output.output_tokens, 80);
    }

    #[test]
    fn test_parse_non_json_lines_skipped() {
        let ndjson =
            "not json at all\n{\"type\":\"result\",\"is_error\":false,\"result\":\"ok\"}\nmore garbage";
        let output = parse_ndjson_from_str(ndjson);
        assert!(!output.is_error);
        assert_eq!(output.result_text.as_deref(), Some("ok"));
    }

    #[test]
    fn test_parse_empty_stream() {
        let output = parse_ndjson_from_str("");
        assert!(output.session_id.is_none());
        assert!(output.text_content.is_empty());
        assert!(output.tool_calls.is_empty());
    }

    #[test]
    fn test_parse_event_valid() {
        let event = parse_event(r#"{"type":"system","session_id":"s1"}"#);
        assert!(event.is_some());
        assert_eq!(event.unwrap().event_type, "system");
    }

    #[test]
    fn test_parse_event_invalid() {
        assert!(parse_event("not json").is_none());
    }

    #[test]
    fn test_parse_event_empty() {
        assert!(parse_event("").is_none());
        assert!(parse_event("   ").is_none());
    }
}

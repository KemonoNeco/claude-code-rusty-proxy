//! Claude CLI subprocess management.
//!
//! Spawns the `claude` binary and parses its NDJSON streaming output.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::cli::types::{ClaudeStreamEvent, UsageInfo};
use crate::config::Config;
use crate::error::ProxyError;

/// Borrowed arguments used to construct a `claude --print` invocation.
pub struct CliArgs<'a> {
    /// The user prompt (everything after `--print`).
    pub prompt: &'a str,
    /// Optional system prompt (`--system-prompt`).
    pub system_prompt: Option<&'a str>,
    /// Resolved Claude model identifier (`--model`).
    pub model: &'a str,
    /// An existing session to resume (`--resume`).
    pub session_id: Option<&'a str>,
    /// Maximum tokens for the response (`--max-tokens`).
    pub max_tokens: Option<u32>,
}

/// Assemble a [`Command`] for `claude --print` with all flags.
///
/// Flags that are always set:
/// * `--output-format stream-json` – NDJSON on stdout
/// * `--verbose` – include usage information in events
/// * `--max-turns 1` – single agentic turn per invocation
///
/// The `CLAUDE_CODE` environment variable is removed so the proxy can
/// itself run inside a Claude Code session without recursion.
fn build_claude_command(args: &CliArgs) -> Command {
    let mut cmd = Command::new("claude");
    cmd.arg("--print")
        .arg(args.prompt)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--model")
        .arg(args.model)
        .arg("--max-turns")
        .arg("1");

    if let Some(system) = args.system_prompt {
        cmd.arg("--system-prompt").arg(system);
    }

    if let Some(sid) = args.session_id {
        cmd.arg("--resume").arg(sid);
    }

    if let Some(mt) = args.max_tokens {
        cmd.arg("--max-tokens").arg(mt.to_string());
    }

    // Allow running inside Claude Code by removing CLAUDE_CODE env var
    cmd.env_remove("CLAUDE_CODE");

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd
}

/// Spawn a [`Command`], translating I/O errors into [`ProxyError`].
fn spawn_command(cmd: &mut Command) -> Result<Child, ProxyError> {
    cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ProxyError::CliNotFound("Claude CLI binary not found".to_string())
        } else {
            ProxyError::CliSpawnFailed(format!("Failed to spawn claude: {}", e))
        }
    })
}

/// Accumulated output from a complete (non-streaming) Claude CLI run.
///
/// Built incrementally by [`process_event`] as NDJSON lines arrive.
#[derive(Debug, Default)]
pub struct CliOutput {
    /// Session ID from the `system` init event, used for `--resume`.
    pub session_id: Option<String>,
    /// Concatenated text from all `assistant` text blocks (joined by `\n`).
    pub text_content: String,
    /// Tool-use blocks extracted from `assistant` events.
    pub tool_calls: Vec<CliToolCall>,
    /// `true` when the `result` event carried `is_error: true`.
    pub is_error: bool,
    /// Total input tokens across all events.
    pub input_tokens: u32,
    /// Total output tokens across all events.
    pub output_tokens: u32,
    /// Plain-text from the `result` event; used as fallback when
    /// `text_content` is empty.
    pub result_text: Option<String>,
}

/// A single tool invocation extracted from a `tool_use` content block.
#[derive(Debug, Clone)]
pub struct CliToolCall {
    /// Unique tool-use ID (e.g. `toolu_01abc`).
    pub id: String,
    /// Tool name (e.g. `Bash`, `Read`).
    pub name: String,
    /// JSON-encoded input parameters.
    pub arguments_json: String,
}

/// Run `claude --print` to completion and return the parsed output.
///
/// Stderr is drained in a background task for debug logging. The whole
/// invocation is wrapped in a [`tokio::time::timeout`] governed by
/// [`Config::timeout`].
pub async fn run_claude(args: &CliArgs<'_>, config: &Config) -> Result<CliOutput, ProxyError> {
    let mut cmd = build_claude_command(args);
    let mut child = spawn_command(&mut cmd)?;

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

/// Spawn `claude --print` and return the child handle + its stdout pipe.
///
/// The caller is responsible for reading NDJSON lines from stdout and
/// for killing/waiting on the child once done.
pub async fn spawn_claude_streaming(
    args: &CliArgs<'_>,
) -> Result<(tokio::process::Child, tokio::process::ChildStdout), ProxyError> {
    let mut cmd = build_claude_command(args);
    let mut child = spawn_command(&mut cmd)?;

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

/// Read all NDJSON lines from `stdout` and fold them into a [`CliOutput`].
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

/// Try to deserialise a single NDJSON line; returns `None` for blank or
/// unparseable lines.
pub fn parse_event(line: &str) -> Option<ClaudeStreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// Fold a single [`ClaudeStreamEvent`] into the running [`CliOutput`].
///
/// * `system` events capture the session ID.
/// * `assistant` events extract text and tool-use blocks.
/// * `result` events capture the final status and optional result text.
/// * Usage tokens are accumulated from every event.
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

/// Sum top-level and message-level usage tokens into `output`.
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
    //! Tests for NDJSON parsing and event processing.
    //!
    //! Uses `parse_ndjson_from_str` to simulate multi-line CLI output and
    //! verify that `CliOutput` is built correctly: session ID capture, text
    //! aggregation, tool-call extraction, usage summation, result flags,
    //! and graceful handling of non-JSON lines.

    use super::*;

    /// Simulate `parse_ndjson_stream` synchronously from a string.
    fn parse_ndjson_from_str(ndjson: &str) -> CliOutput {
        let mut output = CliOutput::default();
        for line in ndjson.lines() {
            if let Some(event) = parse_event(line) {
                process_event(&mut output, &event);
            }
        }
        output
    }

    /// `system` event stores the session ID on the output.
    #[test]
    fn test_parse_system_event_captures_session_id() {
        let ndjson = r#"{"type":"system","session_id":"sess-abc-123","subtype":"init"}
{"type":"result","is_error":false,"result":"Done"}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.session_id.as_deref(), Some("sess-abc-123"));
    }

    /// Multiple assistant text events are joined by newlines.
    #[test]
    fn test_parse_assistant_text_aggregated() {
        let ndjson = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello "}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"world!"}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.text_content, "Hello \nworld!");
    }

    /// Tool-use blocks are extracted into `CliToolCall` with id, name, args.
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

    /// A single event with both text and tool_use blocks populates both.
    #[test]
    fn test_parse_mixed_text_and_tool_use() {
        let ndjson = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Let me check."},{"type":"tool_use","id":"t1","name":"Read","input":{"path":"file.txt"}}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.text_content, "Let me check.");
        assert_eq!(output.tool_calls.len(), 1);
        assert_eq!(output.tool_calls[0].name, "Read");
    }

    /// Successful result captures `is_error: false` and result text.
    #[test]
    fn test_parse_result_success() {
        let ndjson = r#"{"type":"result","is_error":false,"result":"All done.","num_turns":3}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert!(!output.is_error);
        assert_eq!(output.result_text.as_deref(), Some("All done."));
    }

    /// Error result sets `is_error: true`.
    #[test]
    fn test_parse_result_error() {
        let ndjson = r#"{"type":"result","is_error":true,"result":"Something went wrong"}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert!(output.is_error);
    }

    /// Usage tokens across multiple events are summed.
    #[test]
    fn test_parse_usage_info_aggregated() {
        let ndjson = r#"{"type":"assistant","usage":{"input_tokens":100,"output_tokens":50},"message":{"content":[{"type":"text","text":"Hi"}]}}
{"type":"assistant","usage":{"input_tokens":0,"output_tokens":30},"message":{"content":[{"type":"text","text":" there"}]}}
{"type":"result","is_error":false}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert_eq!(output.input_tokens, 100);
        assert_eq!(output.output_tokens, 80);
    }

    /// Non-JSON lines are silently skipped; valid events still parsed.
    #[test]
    fn test_parse_non_json_lines_skipped() {
        let ndjson =
            "not json at all\n{\"type\":\"result\",\"is_error\":false,\"result\":\"ok\"}\nmore garbage";
        let output = parse_ndjson_from_str(ndjson);
        assert!(!output.is_error);
        assert_eq!(output.result_text.as_deref(), Some("ok"));
    }

    /// Empty input produces a default (empty) output.
    #[test]
    fn test_parse_empty_stream() {
        let output = parse_ndjson_from_str("");
        assert!(output.session_id.is_none());
        assert!(output.text_content.is_empty());
        assert!(output.tool_calls.is_empty());
    }

    /// Valid JSON returns `Some(event)`.
    #[test]
    fn test_parse_event_valid() {
        let event = parse_event(r#"{"type":"system","session_id":"s1"}"#);
        assert!(event.is_some());
        assert_eq!(event.unwrap().event_type, "system");
    }

    /// Invalid JSON returns `None`.
    #[test]
    fn test_parse_event_invalid() {
        assert!(parse_event("not json").is_none());
    }

    /// Empty/whitespace-only lines return `None`.
    #[test]
    fn test_parse_event_empty() {
        assert!(parse_event("").is_none());
        assert!(parse_event("   ").is_none());
    }

    // ── Adversarial / safety tests ──────────────────────────────────

    /// Token overflow uses saturating_add, so u32::MAX + 1 stays at MAX.
    #[test]
    fn test_token_overflow_saturating_add() {
        let mut output = CliOutput::default();
        output.input_tokens = u32::MAX;
        output.output_tokens = u32::MAX;

        let ndjson = r#"{"type":"assistant","usage":{"input_tokens":1,"output_tokens":1},"message":{"content":[{"type":"text","text":"hi"}]}}"#;
        if let Some(event) = parse_event(ndjson) {
            process_event(&mut output, &event);
        }
        assert_eq!(output.input_tokens, u32::MAX);
        assert_eq!(output.output_tokens, u32::MAX);
    }

    /// Shell metacharacters in text are parsed safely (no injection).
    #[test]
    fn test_special_chars_in_text_parsed() {
        let ndjson = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"$(rm -rf /) && `whoami` | <script>alert(1)</script>"}]}}"#;
        let output = parse_ndjson_from_str(ndjson);
        assert!(output.text_content.contains("$(rm -rf /)"));
        assert!(output.text_content.contains("<script>"));
    }

    /// Truncation at a multi-byte boundary produces valid UTF-8.
    #[test]
    fn test_truncate_str_multibyte_boundary() {
        let emoji = "Hello 🌍 World"; // 🌍 is 4 bytes
        // Truncating in the middle of the emoji should back up
        let result = truncate_str(emoji, 8); // "Hello " + partial 🌍
        assert!(result.len() <= 8);
        // Must be valid UTF-8
        let _ = result.to_string();

        // Truncation at exact boundary works
        let ascii = "abcdefgh";
        assert_eq!(truncate_str(ascii, 5), "abcde");

        // Truncation beyond length returns the whole string
        assert_eq!(truncate_str(ascii, 100), "abcdefgh");
    }
}

//! Build OpenAI-shaped response objects from Claude CLI output.
//!
//! Provides builders for both the non-streaming [`ChatCompletionResponse`] and
//! the individual SSE [`ChatCompletionChunk`]s (first chunk, content deltas,
//! tool-call deltas, finish, and error).

use crate::cli::subprocess::{CliOutput, CliToolCall};
use crate::types::openai::*;

/// Build a complete [`ChatCompletionResponse`] from collected CLI output.
///
/// * If tool calls are present, `finish_reason` is `"tool_calls"`.
/// * If `text_content` is empty, falls back to `result_text`.
pub fn build_response(request_id: &str, model: &str, output: &CliOutput) -> ChatCompletionResponse {
    let has_tool_calls = !output.tool_calls.is_empty();

    let finish_reason = if output.is_error {
        "stop".to_string()
    } else if has_tool_calls {
        "tool_calls".to_string()
    } else {
        "stop".to_string()
    };

    let content = if output.text_content.is_empty() {
        output.result_text.clone()
    } else {
        Some(output.text_content.clone())
    };

    let tool_calls = if has_tool_calls {
        Some(convert_tool_calls(&output.tool_calls))
    } else {
        None
    };

    ChatCompletionResponse {
        id: request_id.to_string(),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![Choice {
            index: 0,
            message: ChoiceMessage {
                role: "assistant".to_string(),
                content,
                tool_calls,
            },
            finish_reason,
        }],
        usage: Usage {
            prompt_tokens: output.input_tokens,
            completion_tokens: output.output_tokens,
            total_tokens: output.input_tokens.saturating_add(output.output_tokens),
        },
    }
}

/// Build the opening SSE chunk that carries `role: "assistant"` and no content.
pub fn build_first_chunk(request_id: &str, model: &str) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: Some("assistant".to_string()),
                content: None,
                tool_calls: None,
            },
            finish_reason: None,
        }],
        usage: None,
    }
}

/// Build an SSE chunk carrying a text content delta.
pub fn build_content_chunk(request_id: &str, model: &str, text: &str) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: Some(text.to_string()),
                tool_calls: None,
            },
            finish_reason: None,
        }],
        usage: None,
    }
}

/// Build an SSE chunk carrying a tool-call delta (one per tool invocation).
pub fn build_tool_call_chunk(
    request_id: &str,
    model: &str,
    tool_index: u32,
    tool_call: &CliToolCall,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: None,
                tool_calls: Some(vec![ChunkToolCall {
                    index: tool_index,
                    id: Some(tool_call.id.clone()),
                    r#type: Some("function".to_string()),
                    function: ChunkFunctionCall {
                        name: Some(tool_call.name.clone()),
                        arguments: Some(tool_call.arguments_json.clone()),
                    },
                }]),
            },
            finish_reason: None,
        }],
        usage: None,
    }
}

/// Build the final SSE chunk with `finish_reason` and optional accumulated `usage`.
pub fn build_finish_chunk(
    request_id: &str,
    model: &str,
    finish_reason: &str,
    usage: Option<Usage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: None,
                tool_calls: None,
            },
            finish_reason: Some(finish_reason.to_string()),
        }],
        usage,
    }
}

/// Build an SSE content chunk that injects an `[Error: …]` message.
///
/// Sent when the CLI exits with a non-zero status before producing a
/// `result` event.
pub fn build_error_chunk(request_id: &str, model: &str, error_msg: &str) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: Some(format!("\n\n[Error: {}]", error_msg)),
                tool_calls: None,
            },
            finish_reason: None,
        }],
        usage: None,
    }
}

/// Convert CLI tool calls to OpenAI tool call format.
fn convert_tool_calls(cli_calls: &[CliToolCall]) -> Vec<ToolCall> {
    cli_calls
        .iter()
        .map(|c| ToolCall {
            id: c.id.clone(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: c.name.clone(),
                arguments: c.arguments_json.clone(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Tests for CLI-output-to-OpenAI-response conversion.
    //!
    //! Covers text responses, tool-call responses, fallback to `result_text`,
    //! usage mapping, SSE chunk builders (first, content, tool-call, finish,
    //! error), and multi-tool-call / mixed-content edge cases.

    use super::*;

    /// Helper: build a text-only `CliOutput`.
    fn make_text_output(text: &str) -> CliOutput {
        CliOutput {
            session_id: Some("sess-1".to_string()),
            text_content: text.to_string(),
            tool_calls: vec![],
            is_error: false,
            input_tokens: 100,
            output_tokens: 50,
            result_text: None,
        }
    }

    /// Helper: build a tool-call-only `CliOutput`.
    fn make_tool_output() -> CliOutput {
        CliOutput {
            session_id: Some("sess-1".to_string()),
            text_content: String::new(),
            tool_calls: vec![CliToolCall {
                id: "toolu_01".to_string(),
                name: "get_weather".to_string(),
                arguments_json: r#"{"location":"NYC"}"#.to_string(),
            }],
            is_error: false,
            input_tokens: 80,
            output_tokens: 40,
            result_text: None,
        }
    }

    /// Text-only response: content present, `finish_reason: "stop"`, no tool_calls.
    #[test]
    fn test_build_response_text() {
        let output = make_text_output("Hello world!");
        let response = build_response("chatcmpl-123", "claude-sonnet-4", &output);

        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.object, "chat.completion");
        assert_eq!(response.model, "claude-sonnet-4");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Hello world!")
        );
        assert_eq!(response.choices[0].finish_reason, "stop");
        assert!(response.choices[0].message.tool_calls.is_none());
        assert_eq!(response.usage.prompt_tokens, 100);
        assert_eq!(response.usage.completion_tokens, 50);
        assert_eq!(response.usage.total_tokens, 150);
    }

    /// Tool-call response: `finish_reason: "tool_calls"`, content is `None`.
    #[test]
    fn test_build_response_tool_calls() {
        let output = make_tool_output();
        let response = build_response("chatcmpl-456", "claude-sonnet-4", &output);

        assert_eq!(response.choices[0].finish_reason, "tool_calls");
        assert!(response.choices[0].message.content.is_none());
        let tc = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0].id, "toolu_01");
        assert_eq!(tc[0].function.name, "get_weather");
    }

    /// When `text_content` is empty, `result_text` is used as fallback content.
    #[test]
    fn test_build_response_fallback_to_result_text() {
        let output = CliOutput {
            text_content: String::new(),
            result_text: Some("Fallback text".to_string()),
            ..Default::default()
        };
        let response = build_response("chatcmpl-789", "claude-sonnet-4", &output);
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Fallback text")
        );
    }

    /// `input_tokens` maps to `prompt_tokens`; total is their sum.
    #[test]
    fn test_build_response_usage_mapping() {
        let output = CliOutput {
            input_tokens: 200,
            output_tokens: 100,
            ..Default::default()
        };
        let response = build_response("id", "model", &output);
        assert_eq!(response.usage.prompt_tokens, 200);
        assert_eq!(response.usage.completion_tokens, 100);
        assert_eq!(response.usage.total_tokens, 300);
    }

    /// First SSE chunk has `role: "assistant"`, no content, no finish.
    #[test]
    fn test_build_first_chunk() {
        let chunk = build_first_chunk("chatcmpl-123", "claude-sonnet-4");
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.choices[0].delta.role.as_deref(), Some("assistant"));
        assert!(chunk.choices[0].delta.content.is_none());
        assert!(chunk.choices[0].finish_reason.is_none());
        assert!(chunk.usage.is_none());
    }

    /// Content chunk: no role, text in `content`, no finish.
    #[test]
    fn test_build_content_chunk() {
        let chunk = build_content_chunk("chatcmpl-123", "claude-sonnet-4", "Hello");
        assert!(chunk.choices[0].delta.role.is_none());
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    /// Tool-call chunk carries `delta.tool_calls[0]` with id and function.
    #[test]
    fn test_build_tool_call_chunk() {
        let tc = CliToolCall {
            id: "call_1".to_string(),
            name: "search".to_string(),
            arguments_json: r#"{"q":"test"}"#.to_string(),
        };
        let chunk = build_tool_call_chunk("chatcmpl-123", "claude-sonnet-4", 0, &tc);
        let delta_tc = &chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0];
        assert_eq!(delta_tc.id.as_deref(), Some("call_1"));
        assert_eq!(delta_tc.function.name.as_deref(), Some("search"));
    }

    /// Finish chunk with `"stop"` and no usage.
    #[test]
    fn test_build_finish_chunk_stop() {
        let chunk = build_finish_chunk("chatcmpl-123", "claude-sonnet-4", "stop", None);
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        assert!(chunk.choices[0].delta.content.is_none());
        assert!(chunk.choices[0].delta.role.is_none());
        assert!(chunk.usage.is_none());
    }

    /// Finish chunk with accumulated usage present.
    #[test]
    fn test_build_finish_chunk_with_usage() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        };
        let chunk = build_finish_chunk("chatcmpl-123", "claude-sonnet-4", "stop", Some(usage));
        assert!(chunk.usage.is_some());
        assert_eq!(chunk.usage.unwrap().total_tokens, 15);
    }

    /// `convert_tool_calls` maps `CliToolCall` vec to OpenAI `ToolCall` vec.
    #[test]
    fn test_convert_tool_calls() {
        let cli_calls = vec![
            CliToolCall {
                id: "t1".to_string(),
                name: "func1".to_string(),
                arguments_json: "{}".to_string(),
            },
            CliToolCall {
                id: "t2".to_string(),
                name: "func2".to_string(),
                arguments_json: r#"{"key":"val"}"#.to_string(),
            },
        ];
        let openai_calls = convert_tool_calls(&cli_calls);
        assert_eq!(openai_calls.len(), 2);
        assert_eq!(openai_calls[0].r#type, "function");
        assert_eq!(openai_calls[0].function.name, "func1");
        assert_eq!(openai_calls[1].function.name, "func2");
    }

    /// Multiple tool calls in a single response.
    #[test]
    fn test_build_response_multiple_tool_calls() {
        let output = CliOutput {
            text_content: String::new(),
            tool_calls: vec![
                CliToolCall {
                    id: "t1".to_string(),
                    name: "read_file".to_string(),
                    arguments_json: r#"{"path":"a.rs"}"#.to_string(),
                },
                CliToolCall {
                    id: "t2".to_string(),
                    name: "write_file".to_string(),
                    arguments_json: r#"{"path":"b.rs","content":"x"}"#.to_string(),
                },
            ],
            ..Default::default()
        };
        let response = build_response("id", "model", &output);
        assert_eq!(response.choices[0].finish_reason, "tool_calls");
        let tc = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tc.len(), 2);
        assert_eq!(tc[0].function.name, "read_file");
        assert_eq!(tc[1].function.name, "write_file");
        assert_eq!(tc[0].r#type, "function");
        assert_eq!(tc[1].r#type, "function");
    }

    /// When both text and tool calls exist, `finish_reason` is `"tool_calls"`
    /// and content is still present.
    #[test]
    fn test_build_response_mixed_text_and_tools() {
        let output = CliOutput {
            text_content: "Let me check that file.".to_string(),
            tool_calls: vec![CliToolCall {
                id: "t1".to_string(),
                name: "read_file".to_string(),
                arguments_json: r#"{"path":"a.rs"}"#.to_string(),
            }],
            ..Default::default()
        };
        let response = build_response("id", "model", &output);
        // When both text and tool calls exist, finish_reason should be tool_calls
        assert_eq!(response.choices[0].finish_reason, "tool_calls");
        // Content should still be present
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Let me check that file.")
        );
        assert!(response.choices[0].message.tool_calls.is_some());
    }

    /// Error chunk injects `[Error: …]` text into the content delta.
    #[test]
    fn test_build_error_chunk() {
        let chunk = build_error_chunk("chatcmpl-err", "claude-sonnet-4", "CLI exited with code 1");
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.id, "chatcmpl-err");
        let content = chunk.choices[0].delta.content.as_deref().unwrap();
        assert!(content.contains("[Error:"));
        assert!(content.contains("CLI exited with code 1"));
        assert!(chunk.choices[0].finish_reason.is_none());
        assert!(chunk.usage.is_none());
    }
}

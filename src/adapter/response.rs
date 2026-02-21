//! Convert CLI output to OpenAI response/SSE chunk format.

use crate::cli::subprocess::{CliOutput, CliToolCall};
use crate::types::openai::*;

/// Build an OpenAI `ChatCompletionResponse` from CLI output.
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

/// Build the first SSE chunk (with role).
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

/// Build a content delta SSE chunk.
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

/// Build a tool call delta SSE chunk.
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

/// Build the finish SSE chunk.
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
    use super::*;

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

    #[test]
    fn test_build_first_chunk() {
        let chunk = build_first_chunk("chatcmpl-123", "claude-sonnet-4");
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.choices[0].delta.role.as_deref(), Some("assistant"));
        assert!(chunk.choices[0].delta.content.is_none());
        assert!(chunk.choices[0].finish_reason.is_none());
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn test_build_content_chunk() {
        let chunk = build_content_chunk("chatcmpl-123", "claude-sonnet-4", "Hello");
        assert!(chunk.choices[0].delta.role.is_none());
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

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

    #[test]
    fn test_build_finish_chunk_stop() {
        let chunk = build_finish_chunk("chatcmpl-123", "claude-sonnet-4", "stop", None);
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        assert!(chunk.choices[0].delta.content.is_none());
        assert!(chunk.choices[0].delta.role.is_none());
        assert!(chunk.usage.is_none());
    }

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
        let openai_calls = super::convert_tool_calls(&cli_calls);
        assert_eq!(openai_calls.len(), 2);
        assert_eq!(openai_calls[0].r#type, "function");
        assert_eq!(openai_calls[0].function.name, "func1");
        assert_eq!(openai_calls[1].function.name, "func2");
    }
}

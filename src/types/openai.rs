//! OpenAI-compatible wire types for chat completions.
//!
//! All structs derive `Serialize` and/or `Deserialize` so they can be used
//! directly with [`axum::Json`]. Fields that are accepted for API
//! compatibility but not forwarded to the CLI are marked as such.

use serde::{Deserialize, Serialize};

/// Incoming `POST /v1/chat/completions` request body.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model identifier (resolved via [`crate::adapter::model_map`]).
    pub model: String,
    /// Conversation messages (system, user, assistant, tool).
    pub messages: Vec<ChatMessage>,
    /// When `true` the response is streamed as SSE chunks.
    #[serde(default)]
    pub stream: bool,
    /// Accepted for API compat, **not** forwarded to the CLI.
    #[serde(default)]
    #[allow(dead_code)]
    pub temperature: Option<f64>,
    /// Forwarded to `claude --max-tokens`.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Accepted for API compat, **not** forwarded to the CLI.
    #[serde(default)]
    #[allow(dead_code)]
    pub tools: Option<Vec<Tool>>,
    /// Accepted for API compat, **not** forwarded to the CLI.
    #[serde(default)]
    #[allow(dead_code)]
    pub tool_choice: Option<serde_json::Value>,
    /// Optional thread ID used to resume a previous CLI session.
    #[serde(default)]
    pub thread_id: Option<String>,
}

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// One of `"system"`, `"user"`, `"assistant"`, or `"tool"`.
    pub role: String,
    /// Text or multi-part content. May be `null` for tool-call-only messages.
    #[serde(default)]
    pub content: Option<MessageContent>,
    /// Tool name (present on `tool` role messages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls made by the assistant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID of the tool call this message is responding to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Message content — either a plain string or an array of typed parts.
///
/// Deserialised with `#[serde(untagged)]` so both `"hello"` and
/// `[{"type":"text","text":"hello"}]` are accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text string.
    Text(String),
    /// Array of content parts (text, image, etc.).
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    /// Flatten the content into a single text string.
    ///
    /// For [`Parts`](Self::Parts), only `"text"` parts are included and
    /// concatenated without a separator.
    pub fn to_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|p| {
                    if p.r#type == "text" {
                        p.text.as_deref()
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

/// A content part (for multi-modal messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub image_url: Option<serde_json::Value>,
}

/// Tool definition in OpenAI format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: FunctionDef,
}

/// Function definition within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
}

/// A tool call made by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: FunctionCall,
}

/// Function call details within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// OpenAI-compatible chat completion response (non-streaming).
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

/// A single choice in the completion response.
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChoiceMessage,
    pub finish_reason: String,
}

/// The message in a completion choice.
#[derive(Debug, Clone, Serialize)]
pub struct ChoiceMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// OpenAI-compatible streaming chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// A single choice in a streaming chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    pub finish_reason: Option<String>,
}

/// The delta content in a streaming chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChunkToolCall>>,
}

/// Tool call in a streaming chunk delta.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkToolCall {
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    pub function: ChunkFunctionCall,
}

/// Function call fragment in a streaming chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// OpenAI-compatible model list response.
#[derive(Debug, Clone, Serialize)]
pub struct ModelList {
    pub object: String,
    pub data: Vec<Model>,
}

/// A single model entry.
#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

#[cfg(test)]
mod tests {
    //! Tests for OpenAI type serialisation / deserialisation.
    //!
    //! These validate that:
    //! - Requests deserialise correctly from minimal and full JSON payloads.
    //! - The untagged `MessageContent` enum handles both string and array forms.
    //! - Response and chunk structs serialise into the expected OpenAI shape.
    //! - Optional fields (`tool_calls`, `usage`) are omitted when `None`.
    //! - `ToolCall` survives a serialize-then-deserialize round-trip.

    use super::*;

    /// Minimal request: only required fields, all optionals default.
    #[test]
    fn test_request_deserialization_minimal() {
        let json = r#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":"Hello"}]}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "claude-sonnet-4");
        assert_eq!(req.messages.len(), 1);
        assert!(!req.stream);
        assert!(req.temperature.is_none());
        assert!(req.max_tokens.is_none());
        assert!(req.tools.is_none());
    }

    /// The `stream` flag defaults to false; verify it deserialises as true.
    #[test]
    fn test_request_deserialization_with_stream() {
        let json = r#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":"Hi"}],"stream":true}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.stream);
    }

    /// A request carrying a `tools` array should deserialise the function name.
    #[test]
    fn test_request_deserialization_with_tools() {
        let json = r#"{
            "model": "claude-sonnet-4",
            "messages": [{"role": "user", "content": "What's the weather?"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather for a location",
                    "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
                }
            }]
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        let tools = req.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "get_weather");
    }

    /// The custom `thread_id` extension field should deserialise.
    #[test]
    fn test_request_with_thread_id() {
        let json = r#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":"Hi"}],"thread_id":"thread-123"}"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.thread_id.as_deref(), Some("thread-123"));
    }

    /// Simple string content should round-trip through `to_text()`.
    #[test]
    fn test_message_content_text() {
        let json = r#"{"role":"user","content":"Hello"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.unwrap().to_text(), "Hello");
    }

    /// Multi-part content should concatenate text parts.
    #[test]
    fn test_message_content_parts() {
        let json = r#"{"role":"user","content":[{"type":"text","text":"Hello "},{"type":"text","text":"world"}]}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.unwrap().to_text(), "Hello world");
    }

    /// `content: null` with `tool_calls` present (assistant tool-call message).
    #[test]
    fn test_message_content_null() {
        let json = r#"{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"test","arguments":"{}"}}]}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
    }

    /// Tool-result message carries `role: "tool"` and a `tool_call_id`.
    #[test]
    fn test_message_with_tool_call_id() {
        let json = r#"{"role":"tool","content":"result data","tool_call_id":"call_1"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_1"));
    }

    /// Non-streaming response serialises with the expected OpenAI fields;
    /// `tool_calls` is absent when `None`.
    #[test]
    fn test_response_serialization() {
        let response = ChatCompletionResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1700000000,
            model: "claude-sonnet-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChoiceMessage {
                    role: "assistant".to_string(),
                    content: Some("Hello!".to_string()),
                    tool_calls: None,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["id"], "chatcmpl-123");
        assert_eq!(json["object"], "chat.completion");
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
        assert_eq!(json["usage"]["total_tokens"], 15);
        // tool_calls should be absent when None
        assert!(json["choices"][0]["message"].get("tool_calls").is_none());
    }

    /// Response with `tool_calls` should have `finish_reason: "tool_calls"`.
    #[test]
    fn test_response_with_tool_calls() {
        let response = ChatCompletionResponse {
            id: "chatcmpl-456".to_string(),
            object: "chat.completion".to_string(),
            created: 1700000000,
            model: "claude-sonnet-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChoiceMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_1".to_string(),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name: "get_weather".to_string(),
                            arguments: r#"{"location":"NYC"}"#.to_string(),
                        },
                    }]),
                },
                finish_reason: "tool_calls".to_string(),
            }],
            usage: Usage::default(),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["choices"][0]["finish_reason"], "tool_calls");
        let tc = &json["choices"][0]["message"]["tool_calls"][0];
        assert_eq!(tc["function"]["name"], "get_weather");
    }

    /// First SSE chunk: carries `role` but no content or finish_reason.
    #[test]
    fn test_chunk_serialization_first_chunk() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1700000000,
            model: "claude-sonnet-4".to_string(),
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
        };
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["object"], "chat.completion.chunk");
        assert_eq!(json["choices"][0]["delta"]["role"], "assistant");
        assert!(json["choices"][0]["finish_reason"].is_null());
        assert!(json.get("usage").is_none());
    }

    /// Content-delta chunk: `role` absent, `content` present.
    #[test]
    fn test_chunk_serialization_content_delta() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1700000000,
            model: "claude-sonnet-4".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: None,
                    content: Some("Hello".to_string()),
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        };
        let json = serde_json::to_value(&chunk).unwrap();
        assert!(json["choices"][0]["delta"].get("role").is_none());
        assert_eq!(json["choices"][0]["delta"]["content"], "Hello");
    }

    /// Finish chunk: `finish_reason` set, `usage` present.
    #[test]
    fn test_chunk_serialization_finish() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1700000000,
            model: "claude-sonnet-4".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: None,
                    content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
        assert_eq!(json["usage"]["total_tokens"], 15);
    }

    /// Tool-call chunk carries `delta.tool_calls` with id, type, and function.
    #[test]
    fn test_chunk_with_tool_calls() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1700000000,
            model: "claude-sonnet-4".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![ChunkToolCall {
                        index: 0,
                        id: Some("call_1".to_string()),
                        r#type: Some("function".to_string()),
                        function: ChunkFunctionCall {
                            name: Some("get_weather".to_string()),
                            arguments: Some(r#"{"location":"NYC"}"#.to_string()),
                        },
                    }]),
                },
                finish_reason: None,
            }],
            usage: None,
        };
        let json = serde_json::to_value(&chunk).unwrap();
        let tc = &json["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(tc["id"], "call_1");
        assert_eq!(tc["function"]["name"], "get_weather");
    }

    /// `ModelList` serialises with `object: "list"` and a `data` array.
    #[test]
    fn test_model_list_serialization() {
        let list = ModelList {
            object: "list".to_string(),
            data: vec![
                Model {
                    id: "claude-opus-4".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "anthropic".to_string(),
                },
                Model {
                    id: "claude-sonnet-4".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "anthropic".to_string(),
                },
            ],
        };
        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"].as_array().unwrap().len(), 2);
        assert_eq!(json["data"][0]["id"], "claude-opus-4");
    }

    /// `Usage::default()` should zero-initialise all counters.
    #[test]
    fn test_usage_default() {
        let usage = Usage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    /// Serialize then deserialize a `ToolCall` to verify serde round-trip.
    #[test]
    fn test_tool_call_roundtrip() {
        let tc = ToolCall {
            id: "call_abc".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "search".to_string(),
                arguments: r#"{"q":"test"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tc).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "call_abc");
        assert_eq!(parsed.function.name, "search");
    }

    // ── Adversarial deserialization tests ────────────────────────────

    /// Missing `model` field must produce a serde error.
    #[test]
    fn test_deserialize_missing_model() {
        let json = r#"{"messages":[{"role":"user","content":"Hi"}]}"#;
        let result = serde_json::from_str::<ChatCompletionRequest>(json);
        assert!(result.is_err());
    }

    /// `stream` as a string instead of bool must produce a serde error.
    #[test]
    fn test_deserialize_wrong_type_stream() {
        let json = r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}],"stream":"yes"}"#;
        let result = serde_json::from_str::<ChatCompletionRequest>(json);
        assert!(result.is_err());
    }

    /// Unknown fields are silently ignored (no `deny_unknown_fields`).
    #[test]
    fn test_deserialize_extra_fields_ignored() {
        let json = r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}],"unknown_field":"value","another":42}"#;
        let result = serde_json::from_str::<ChatCompletionRequest>(json);
        assert!(result.is_ok());
    }

    /// Negative `max_tokens` must fail since it's `Option<u32>`.
    #[test]
    fn test_deserialize_negative_max_tokens() {
        let json = r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}],"max_tokens":-1}"#;
        let result = serde_json::from_str::<ChatCompletionRequest>(json);
        assert!(result.is_err());
    }

    /// `max_tokens` exceeding u32::MAX must fail.
    #[test]
    fn test_deserialize_overflow_max_tokens() {
        let json = r#"{"model":"sonnet","messages":[{"role":"user","content":"Hi"}],"max_tokens":5000000000}"#;
        let result = serde_json::from_str::<ChatCompletionRequest>(json);
        assert!(result.is_err());
    }

    /// Empty content parts array deserialises ok and yields empty text.
    #[test]
    fn test_empty_content_parts_array() {
        let json = r#"{"role":"user","content":[]}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.unwrap().to_text(), "");
    }

    /// Non-text content parts (e.g. image) are skipped in `to_text()`.
    #[test]
    fn test_non_text_content_parts() {
        let json = r#"{"role":"user","content":[{"type":"image_url","image_url":{"url":"https://example.com/img.png"}}]}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.unwrap().to_text(), "");
    }
}

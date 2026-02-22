//! Convert an OpenAI messages array into a flat text prompt for `claude --print`.
//!
//! The conversion rules:
//!
//! | Role        | Treatment |
//! |-------------|-----------|
//! | `system`    | Extracted into a separate `--system-prompt` string. |
//! | `user`      | Appended verbatim to the prompt. |
//! | `assistant` | Wrapped in `[Assistant: …]` or `[Assistant used tools: …]`. |
//! | `tool`      | Wrapped in `[Tool result for <name> (<id>): …]`. |
//!
//! Multiple parts are joined by double-newlines.

use crate::types::openai::ChatMessage;

/// Split an OpenAI messages array into `(system_prompt, user_prompt)`.
///
/// System messages are joined separately (for `--system-prompt`). All other
/// roles are flattened into a single prompt string for `--print`.
pub fn convert_messages(messages: &[ChatMessage]) -> (Option<String>, String) {
    let mut system_parts = Vec::new();
    let mut prompt_parts = Vec::new();

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                if let Some(ref content) = msg.content {
                    let text = content.to_text();
                    if !text.is_empty() {
                        system_parts.push(text);
                    }
                }
            }
            "user" => {
                if let Some(ref content) = msg.content {
                    let text = content.to_text();
                    if !text.is_empty() {
                        prompt_parts.push(text);
                    }
                }
            }
            "assistant" => {
                if let Some(ref tool_calls) = msg.tool_calls {
                    let calls_desc: Vec<String> = tool_calls
                        .iter()
                        .map(|c| format!("{}({})", c.function.name, c.function.arguments))
                        .collect();

                    let content_text = msg
                        .content
                        .as_ref()
                        .map(|c| c.to_text())
                        .unwrap_or_default();

                    if content_text.is_empty() {
                        prompt_parts
                            .push(format!("[Assistant used tools: {}]", calls_desc.join(", ")));
                    } else {
                        prompt_parts.push(format!(
                            "{}\n[Assistant used tools: {}]",
                            content_text,
                            calls_desc.join(", ")
                        ));
                    }
                } else if let Some(ref content) = msg.content {
                    let text = content.to_text();
                    if !text.is_empty() {
                        prompt_parts.push(format!("[Assistant: {}]", text));
                    }
                }
            }
            "tool" => {
                let name = msg.name.as_deref().unwrap_or("tool");
                let call_id = msg.tool_call_id.as_deref().unwrap_or("unknown");
                let content_text = msg
                    .content
                    .as_ref()
                    .map(|c| c.to_text())
                    .unwrap_or_default();
                prompt_parts.push(format!(
                    "[Tool result for {} ({}): {}]",
                    name, call_id, content_text
                ));
            }
            _ => {}
        }
    }

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    let prompt = prompt_parts.join("\n\n");
    (system, prompt)
}

#[cfg(test)]
mod tests {
    //! Tests for OpenAI-to-CLI message conversion.
    //!
    //! Verifies system prompt extraction, user content passthrough, assistant
    //! text wrapping, tool-call summarisation, tool-result formatting, unknown
    //! role skipping, and empty-content elision.

    use super::*;
    use crate::types::openai::{ChatMessage, FunctionCall, MessageContent, ToolCall};

    /// Helper: build a user message.
    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Helper: build a system message.
    fn system_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "system".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Helper: build an assistant text message.
    fn assistant_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Helper: build an assistant message with tool calls and no text.
    fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: None,
            name: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Helper: build a tool-result message.
    fn tool_msg(call_id: &str, name: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: Some(name.to_string()),
            tool_calls: None,
            tool_call_id: Some(call_id.to_string()),
        }
    }

    /// System messages are separated into the system prompt; user -> prompt.
    #[test]
    fn test_system_extraction() {
        let messages = vec![system_msg("You are helpful."), user_msg("Hello")];
        let (system, prompt) = convert_messages(&messages);
        assert_eq!(system.as_deref(), Some("You are helpful."));
        assert_eq!(prompt, "Hello");
    }

    /// Multiple system messages are joined by double-newlines.
    #[test]
    fn test_multi_system_concat() {
        let messages = vec![
            system_msg("System 1"),
            system_msg("System 2"),
            user_msg("Hello"),
        ];
        let (system, prompt) = convert_messages(&messages);
        assert_eq!(system.as_deref(), Some("System 1\n\nSystem 2"));
        assert_eq!(prompt, "Hello");
    }

    /// Single user message with no system produces `None` system and prompt.
    #[test]
    fn test_single_user() {
        let messages = vec![user_msg("What is 2+2?")];
        let (system, prompt) = convert_messages(&messages);
        assert!(system.is_none());
        assert_eq!(prompt, "What is 2+2?");
    }

    /// Assistant text is wrapped in `[Assistant: …]` brackets.
    #[test]
    fn test_assistant_with_text() {
        let messages = vec![
            user_msg("Hello"),
            assistant_msg("Hi there!"),
            user_msg("How are you?"),
        ];
        let (system, prompt) = convert_messages(&messages);
        assert!(system.is_none());
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains("[Assistant: Hi there!]"));
        assert!(prompt.contains("How are you?"));
    }

    /// Assistant tool calls are summarised as `[Assistant used tools: …]`.
    #[test]
    fn test_assistant_with_tool_calls() {
        let messages = vec![
            user_msg("List files"),
            assistant_with_tools(vec![ToolCall {
                id: "call_1".to_string(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "shell".to_string(),
                    arguments: r#"{"command":"ls"}"#.to_string(),
                },
            }]),
        ];
        let (_system, prompt) = convert_messages(&messages);
        assert!(prompt.contains("[Assistant used tools:"));
        assert!(prompt.contains("shell"));
    }

    /// Tool results are formatted as `[Tool result for <name> (<id>): …]`.
    #[test]
    fn test_tool_results() {
        let messages = vec![
            user_msg("List files"),
            tool_msg("call_1", "shell", "file1.txt\nfile2.txt"),
        ];
        let (_system, prompt) = convert_messages(&messages);
        assert!(prompt.contains("[Tool result for shell (call_1):"));
        assert!(prompt.contains("file1.txt"));
    }

    /// Empty message array produces `None` system and empty prompt.
    #[test]
    fn test_empty_messages() {
        let messages: Vec<ChatMessage> = vec![];
        let (system, prompt) = convert_messages(&messages);
        assert!(system.is_none());
        assert!(prompt.is_empty());
    }

    /// Full conversation: system + user + assistant + user.
    #[test]
    fn test_mixed_conversation() {
        let messages = vec![
            system_msg("Be concise."),
            user_msg("Hello"),
            assistant_msg("Hi!"),
            user_msg("What time is it?"),
        ];
        let (system, prompt) = convert_messages(&messages);
        assert_eq!(system.as_deref(), Some("Be concise."));
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains("[Assistant: Hi!]"));
        assert!(prompt.contains("What time is it?"));
    }

    /// Messages with unrecognised roles (e.g. `developer`) are silently dropped.
    #[test]
    fn test_unknown_role_ignored() {
        let messages = vec![
            user_msg("Hello"),
            ChatMessage {
                role: "developer".to_string(),
                content: Some(MessageContent::Text("secret".to_string())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            user_msg("World"),
        ];
        let (_, prompt) = convert_messages(&messages);
        assert!(!prompt.contains("secret"));
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains("World"));
    }

    /// Messages with empty string content are skipped (no extra separators).
    #[test]
    fn test_empty_content_skipped() {
        let messages = vec![
            user_msg("Hello"),
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text(String::new())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            user_msg("World"),
        ];
        let (_, prompt) = convert_messages(&messages);
        // Empty content should not add extra separators
        assert_eq!(prompt, "Hello\n\nWorld");
    }

    // ── Adversarial / edge case tests ───────────────────────────────

    /// Tool name containing brackets doesn't confuse the formatting.
    #[test]
    fn test_tool_name_with_brackets() {
        let messages = vec![
            user_msg("Run it"),
            tool_msg("call_1", "get[data]", "result"),
        ];
        let (_, prompt) = convert_messages(&messages);
        assert!(prompt.contains("[Tool result for get[data] (call_1): result]"));
    }

    /// Empty tool arguments string doesn't crash.
    #[test]
    fn test_empty_tool_arguments() {
        let messages = vec![
            user_msg("Do it"),
            assistant_with_tools(vec![ToolCall {
                id: "call_1".to_string(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "noop".to_string(),
                    arguments: String::new(),
                },
            }]),
        ];
        let (_, prompt) = convert_messages(&messages);
        assert!(prompt.contains("[Assistant used tools: noop()]"));
    }

    /// All messages with empty content produce an empty prompt.
    #[test]
    fn test_all_messages_empty_content() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(MessageContent::Text(String::new())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(MessageContent::Text(String::new())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let (system, prompt) = convert_messages(&messages);
        assert!(system.is_none());
        assert!(prompt.is_empty());
    }

    /// Nested brackets in assistant text are preserved correctly.
    #[test]
    fn test_nested_brackets_in_assistant() {
        let messages = vec![
            user_msg("Hello"),
            assistant_msg("Use [brackets] and [[nested]]"),
            user_msg("Thanks"),
        ];
        let (_, prompt) = convert_messages(&messages);
        assert!(prompt.contains("[Assistant: Use [brackets] and [[nested]]]"));
    }
}

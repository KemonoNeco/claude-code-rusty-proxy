//! Convert OpenAI-format messages to Claude CLI prompt format.

use crate::types::openai::ChatMessage;

/// Convert OpenAI messages to a CLI prompt and optional system prompt.
///
/// Extracts system messages separately. Combines user/assistant/tool messages
/// into a prompt suitable for `--print`.
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
    use super::*;
    use crate::types::openai::{ChatMessage, FunctionCall, MessageContent, ToolCall};

    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn system_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "system".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn assistant_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: None,
            name: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    fn tool_msg(call_id: &str, name: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: Some(MessageContent::Text(content.to_string())),
            name: Some(name.to_string()),
            tool_calls: None,
            tool_call_id: Some(call_id.to_string()),
        }
    }

    #[test]
    fn test_system_extraction() {
        let messages = vec![system_msg("You are helpful."), user_msg("Hello")];
        let (system, prompt) = convert_messages(&messages);
        assert_eq!(system.as_deref(), Some("You are helpful."));
        assert_eq!(prompt, "Hello");
    }

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

    #[test]
    fn test_single_user() {
        let messages = vec![user_msg("What is 2+2?")];
        let (system, prompt) = convert_messages(&messages);
        assert!(system.is_none());
        assert_eq!(prompt, "What is 2+2?");
    }

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

    #[test]
    fn test_empty_messages() {
        let messages: Vec<ChatMessage> = vec![];
        let (system, prompt) = convert_messages(&messages);
        assert!(system.is_none());
        assert!(prompt.is_empty());
    }

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
}

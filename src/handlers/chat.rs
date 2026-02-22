//! `POST /v1/chat/completions` – the core endpoint.
//!
//! Handles both **non-streaming** (returns a single JSON response) and
//! **streaming** (returns a Server-Sent Events stream of `chat.completion.chunk`
//! objects) modes.
//!
//! ## Request flow
//!
//! 1. Validate the incoming [`ChatCompletionRequest`].
//! 2. Resolve the model name to a concrete Claude model ID.
//! 3. Convert the OpenAI messages array into a CLI-compatible prompt.
//! 4. Look up an existing Claude CLI session (for `--resume`).
//! 5. Either collect full output (non-streaming) or open an SSE stream.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::Stream;
use tokio::io::AsyncBufReadExt;

use crate::adapter::model_map::resolve_model;
use crate::adapter::request::convert_messages;
use crate::adapter::response;
use crate::cli::subprocess::{self, CliArgs};
use crate::config::Config;
use crate::error::ProxyError;
use crate::session::SessionManager;
use crate::types::openai::{ChatCompletionRequest, Usage};

/// Shared application state injected into every handler via Axum's
/// [`State`](axum::extract::State) extractor.
pub struct AppState {
    pub config: Config,
    pub session_manager: SessionManager,
}

/// `POST /v1/chat/completions`
///
/// Dispatches to either the non-streaming or SSE-streaming path based on
/// `request.stream`.
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ProxyError> {
    let started = Instant::now();
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let message_count = request.messages.len();

    // Validate request
    if request.messages.is_empty() {
        tracing::warn!(request_id = %request_id, "rejected: \"messages array must not be empty\"");
        return Err(ProxyError::InvalidRequest(
            "messages array must not be empty".to_string(),
        ));
    }

    // Resolve model
    let model = resolve_model(&request.model, &state.config.default_model);

    // Convert messages
    let (system_prompt, prompt) = convert_messages(&request.messages);
    if prompt.is_empty() {
        tracing::warn!(request_id = %request_id, "rejected: \"No user content in messages\"");
        return Err(ProxyError::InvalidRequest(
            "No user content in messages".to_string(),
        ));
    }

    // Validate thread_id if present
    if let Some(ref tid) = request.thread_id {
        if tid.contains('\0') {
            return Err(ProxyError::InvalidRequest(
                "thread_id must not contain null bytes".to_string(),
            ));
        }
        if tid.contains("..") {
            return Err(ProxyError::InvalidRequest(
                "thread_id must not contain path traversal sequences".to_string(),
            ));
        }
    }

    tracing::info!(
        request_id = %request_id,
        model = %model.display_name,
        stream = request.stream,
        messages = message_count,
        "chat request"
    );

    // Get session for resume
    let session_id = request
        .thread_id
        .as_deref()
        .and_then(|tid| state.session_manager.get(tid));

    if request.stream {
        // Streaming response
        let stream = create_sse_stream(
            SseParams {
                request_id: request_id.clone(),
                model_id: model.id.to_string(),
                model_display: model.display_name.to_string(),
                prompt,
                system_prompt,
                session_id,
                thread_id: request.thread_id.clone(),
                max_tokens: request.max_tokens,
            },
            state,
        )
        .await?;

        tracing::info!(request_id = %request_id, "streaming started");

        Ok(Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
            .into_response())
    } else {
        // Non-streaming response
        let cli_args = CliArgs {
            prompt: &prompt,
            system_prompt: system_prompt.as_deref(),
            model: model.id,
            session_id: session_id.as_deref(),
            max_tokens: request.max_tokens,
        };
        let output = match subprocess::run_claude(&cli_args, &state.config).await {
            Ok(output) => output,
            Err(e) => {
                tracing::error!(request_id = %request_id, "cli_error: \"{}\"", e);
                return Err(e);
            }
        };

        // Store session for resume
        if let Some(ref tid) = request.thread_id {
            if let Some(ref sid) = output.session_id {
                state.session_manager.store(tid, sid.clone());
            }
        }

        let duration_ms = started.elapsed().as_millis();
        tracing::info!(
            request_id = %request_id,
            input_tokens = output.input_tokens,
            output_tokens = output.output_tokens,
            duration_ms = duration_ms,
            "completed"
        );

        let resp = response::build_response(&request_id, model.display_name, &output);
        Ok(Json(resp).into_response())
    }
}

/// All the data needed to start an SSE stream, moved into the async
/// closure that produces chunks.
struct SseParams {
    request_id: String,
    model_id: String,
    model_display: String,
    prompt: String,
    system_prompt: Option<String>,
    session_id: Option<String>,
    thread_id: Option<String>,
    max_tokens: Option<u32>,
}

/// Spawn the Claude CLI in streaming mode and return an async [`Stream`]
/// of SSE [`Event`]s.
///
/// The stream emits:
/// * One initial chunk with `role: "assistant"`.
/// * Content delta chunks for each text block.
/// * Tool-call delta chunks for each tool-use block.
/// * A finish chunk with `finish_reason` and accumulated `usage`.
/// * A final `[DONE]` sentinel.
async fn create_sse_stream(
    params: SseParams,
    state: Arc<AppState>,
) -> Result<impl Stream<Item = Result<Event, Infallible>>, ProxyError> {
    let cli_args = CliArgs {
        prompt: &params.prompt,
        system_prompt: params.system_prompt.as_deref(),
        model: &params.model_id,
        session_id: params.session_id.as_deref(),
        max_tokens: params.max_tokens,
    };
    let (mut child, stdout) = subprocess::spawn_claude_streaming(&cli_args).await?;

    let request_id = params.request_id;
    let model_display = params.model_display;
    let thread_id = params.thread_id;

    let stream = async_stream::stream! {
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut accumulated_input_tokens: u32 = 0;
        let mut accumulated_output_tokens: u32 = 0;
        let mut captured_session_id: Option<String> = None;
        let mut got_result = false;

        // Send first chunk with role
        let first_chunk = response::build_first_chunk(&request_id, &model_display);
        if let Ok(json) = serde_json::to_string(&first_chunk) {
            yield Ok(Event::default().data(json));
        }

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }

                    let event = match subprocess::parse_event(&line) {
                        Some(e) => e,
                        None => continue,
                    };

                    // Accumulate usage
                    if let Some(ref usage) = event.usage {
                        accumulated_input_tokens = accumulated_input_tokens.saturating_add(usage.input_tokens);
                        accumulated_output_tokens = accumulated_output_tokens.saturating_add(usage.output_tokens);
                    }
                    if let Some(ref msg) = event.message {
                        if let Some(ref usage) = msg.usage {
                            accumulated_input_tokens = accumulated_input_tokens.saturating_add(usage.input_tokens);
                            accumulated_output_tokens = accumulated_output_tokens.saturating_add(usage.output_tokens);
                        }
                    }

                    match event.event_type.as_str() {
                        "system" => {
                            if let Some(ref sid) = event.session_id {
                                captured_session_id = Some(sid.clone());
                            }
                        }
                        "assistant" => {
                            if let Some(ref msg) = event.message {
                                if let Some(ref blocks) = msg.content {
                                    let mut tool_index: u32 = 0;
                                    for block in blocks {
                                        match block.block_type.as_str() {
                                            "text" => {
                                                if let Some(ref text) = block.text {
                                                    if !text.is_empty() {
                                                        let chunk = response::build_content_chunk(
                                                            &request_id,
                                                            &model_display,
                                                            text,
                                                        );
                                                        if let Ok(json) = serde_json::to_string(&chunk) {
                                                            yield Ok(Event::default().data(json));
                                                        }
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
                                                    let cli_tc = subprocess::CliToolCall {
                                                        id: id.clone(),
                                                        name: name.clone(),
                                                        arguments_json: args,
                                                    };
                                                    let chunk = response::build_tool_call_chunk(
                                                        &request_id,
                                                        &model_display,
                                                        tool_index,
                                                        &cli_tc,
                                                    );
                                                    if let Ok(json) = serde_json::to_string(&chunk) {
                                                        yield Ok(Event::default().data(json));
                                                    }
                                                    tool_index += 1;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        "result" => {
                            got_result = true;

                            let has_tool_calls = event.message.as_ref()
                                .and_then(|m| m.content.as_ref())
                                .map(|blocks| blocks.iter().any(|b| b.block_type == "tool_use"))
                                .unwrap_or(false);

                            let finish_reason = if has_tool_calls { "tool_calls" } else { "stop" };

                            let usage = Usage {
                                prompt_tokens: accumulated_input_tokens,
                                completion_tokens: accumulated_output_tokens,
                                total_tokens: accumulated_input_tokens.saturating_add(accumulated_output_tokens),
                            };

                            let finish_chunk = response::build_finish_chunk(
                                &request_id,
                                &model_display,
                                finish_reason,
                                Some(usage),
                            );
                            if let Ok(json) = serde_json::to_string(&finish_chunk) {
                                yield Ok(Event::default().data(json));
                            }

                            // Send [DONE] sentinel
                            yield Ok(Event::default().data("[DONE]"));
                        }
                        _ => {}
                    }
                }
                Ok(None) => break, // EOF
                Err(e) => {
                    tracing::error!("Error reading CLI stdout: {}", e);
                    break;
                }
            }
        }

        // Check exit status after stream ends
        if !got_result {
            if let Ok(status) = child.wait().await {
                if !status.success() {
                    let code = status.code().unwrap_or(-1);
                    let error_chunk = response::build_error_chunk(
                        &request_id,
                        &model_display,
                        &format!("CLI exited with code {}", code),
                    );
                    if let Ok(json) = serde_json::to_string(&error_chunk) {
                        yield Ok(Event::default().data(json));
                    }
                }
            }

            // Send finish + [DONE] if we never got a result event
            let finish_chunk = response::build_finish_chunk(
                &request_id,
                &model_display,
                "stop",
                None,
            );
            if let Ok(json) = serde_json::to_string(&finish_chunk) {
                yield Ok(Event::default().data(json));
            }
            yield Ok(Event::default().data("[DONE]"));
        }

        // Store session for future resume
        if let (Some(tid), Some(sid)) = (&thread_id, &captured_session_id) {
            state.session_manager.store(tid, sid.clone());
        }

        // Ensure child process is cleaned up
        let _ = child.kill().await;
    };

    Ok(stream)
}

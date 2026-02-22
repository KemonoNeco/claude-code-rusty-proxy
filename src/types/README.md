# types/

Wire-format type definitions used throughout the proxy.

## Modules

### [`openai.rs`](openai.rs)

Complete set of Serde structs for the OpenAI chat completions API.

**Request types:**
- `ChatCompletionRequest` — incoming POST body with model, messages, stream flag, max_tokens, thread_id, and compatibility-only fields (temperature, tools, tool_choice).
- `ChatMessage` — a single message with role, content (text or parts), optional tool_calls, and tool_call_id.
- `MessageContent` — untagged enum: plain `String` or `Vec<ContentPart>`.
- `ContentPart` — typed content part (text, image_url).
- `Tool` / `FunctionDef` — tool definitions (accepted for compat, not forwarded).
- `ToolCall` / `FunctionCall` — tool calls made by the assistant.

**Response types (non-streaming):**
- `ChatCompletionResponse` — id, object, created, model, choices, usage.
- `Choice` / `ChoiceMessage` — single choice with message and finish_reason.
- `Usage` — prompt_tokens, completion_tokens, total_tokens.

**Response types (streaming):**
- `ChatCompletionChunk` — SSE chunk with choices and optional usage.
- `ChunkChoice` / `ChunkDelta` — delta with optional role, content, tool_calls.
- `ChunkToolCall` / `ChunkFunctionCall` — tool-call fragments in streaming.

**Model types:**
- `ModelList` / `Model` — `GET /v1/models` response format.

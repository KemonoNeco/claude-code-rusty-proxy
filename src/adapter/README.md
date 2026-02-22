# adapter/

Translation layer between the OpenAI wire format and the Claude CLI.

## Modules

### [`model_map.rs`](model_map.rs)

Resolves flexible model name strings to concrete `ClaudeModel` values.

**Resolution rules** (case-insensitive, whitespace-trimmed):

| Input | Resolves to |
|-------|-------------|
| `sonnet`, `claude-sonnet-4`, `claude-sonnet-4-20250514` | Sonnet |
| `opus`, `claude-opus-4`, `claude-opus-4-20250514` | Opus |
| `haiku`, `claude-haiku-4`, `claude-haiku-4-20250506` | Haiku |
| Anything else | Configured default (recursive, terminates at `sonnet`) |

### [`request.rs`](request.rs)

Converts an OpenAI messages array into a `(system_prompt, user_prompt)` tuple for `claude --print`.

**Conversion rules:**

| Role | Treatment |
|------|-----------|
| `system` | Extracted into `--system-prompt` (multiple joined by `\n\n`) |
| `user` | Appended verbatim to the prompt |
| `assistant` | Wrapped in `[Assistant: …]` or `[Assistant used tools: …]` |
| `tool` | Wrapped in `[Tool result for <name> (<id>): …]` |
| Unknown roles | Silently dropped |

Empty content is skipped. Prompt parts are joined by double-newlines.

### [`response.rs`](response.rs)

Converts CLI output into OpenAI response objects.

**Non-streaming** (`build_response`):
- Builds a `ChatCompletionResponse` from `CliOutput`.
- If tool calls are present, `finish_reason` is `"tool_calls"`.
- If `text_content` is empty, falls back to `result_text`.

**Streaming** (SSE chunk builders):
- `build_first_chunk` — opening chunk with `role: "assistant"`.
- `build_content_chunk` — text content delta.
- `build_tool_call_chunk` — tool-call delta.
- `build_finish_chunk` — final chunk with `finish_reason` and optional usage.
- `build_error_chunk` — injects `[Error: …]` on CLI failure.

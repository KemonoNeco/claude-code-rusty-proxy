# claude-code-rusty-proxy

An OpenAI-compatible API proxy that wraps the [Claude CLI](https://docs.anthropic.com/en/docs/claude-code), allowing any tool or library that speaks the OpenAI chat completions protocol to use Claude Code as its backend.

## How it works

The proxy starts an HTTP server exposing OpenAI-shaped endpoints. When a chat completion request arrives, it:

1. Translates OpenAI messages (system / user / assistant / tool) into a single prompt
2. Spawns `claude --print` as a subprocess with `--output-format stream-json`
3. Parses the NDJSON event stream from the CLI
4. Converts events back into OpenAI response objects (or SSE chunks for streaming)

Multi-turn conversations are supported via a `thread_id` field in the request, which maps to Claude CLI's `--resume` flag internally.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.70+)
- [Claude CLI](https://docs.anthropic.com/en/docs/claude-code) installed and authenticated (`claude` must be on your `PATH`)

## Installation

```bash
git clone https://github.com/<your-org>/claude-code-rusty-proxy.git
cd claude-code-rusty-proxy
cargo build --release
```

The binary will be at `target/release/claude-code-rusty-proxy`.

## Usage

```bash
# Start with defaults (127.0.0.1:3456, sonnet model)
claude-code-rusty-proxy

# Custom port and model
claude-code-rusty-proxy --port 8080 --default-model opus

# Bind to all interfaces with verbose logging
claude-code-rusty-proxy --host 0.0.0.0 --verbose
```

### CLI options

| Flag | Env var | Default | Description |
|------|---------|---------|-------------|
| `--port` | `PROXY_PORT` | `3456` | Port to listen on |
| `--host` | `PROXY_HOST` | `127.0.0.1` | Host to bind to |
| `--timeout` | `PROXY_TIMEOUT` | `300` | Request timeout in seconds |
| `--default-model` | `PROXY_DEFAULT_MODEL` | `sonnet` | Default Claude model when not specified in the request |
| `--verbose` | `PROXY_VERBOSE` | `false` | Enable debug logging |

## API endpoints

### `GET /health`

Health check. Returns:

```json
{ "status": "ok", "provider": "claude-code-rusty-proxy" }
```

### `GET /v1/models`

Lists available models in OpenAI format:

```json
{
  "object": "list",
  "data": [
    { "id": "claude-opus-4", "object": "model", "owned_by": "anthropic", "created": 1700000000 },
    { "id": "claude-sonnet-4", "object": "model", "owned_by": "anthropic", "created": 1700000000 },
    { "id": "claude-haiku-4", "object": "model", "owned_by": "anthropic", "created": 1700000000 }
  ]
}
```

### `POST /v1/chat/completions`

OpenAI-compatible chat completion. Supports both streaming (`"stream": true`) and non-streaming modes.

**Request body:**

```json
{
  "model": "claude-sonnet-4",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user", "content": "Hello!" }
  ],
  "stream": false,
  "max_tokens": 4096,
  "thread_id": "optional-thread-id-for-multi-turn"
}
```

**Model name resolution** is flexible. All of these work:

| Input | Resolves to |
|-------|-------------|
| `sonnet`, `claude-sonnet-4`, `claude-sonnet-4-20250514` | Claude Sonnet 4 |
| `opus`, `claude-opus-4`, `claude-opus-4-20250514` | Claude Opus 4 |
| `haiku`, `claude-haiku-4`, `claude-haiku-4-20250506` | Claude Haiku 4 |
| Unrecognized names (e.g. `gpt-4o`) | Falls back to `--default-model` |

**Supported request fields:**

| Field | Forwarded to CLI | Notes |
|-------|-----------------|-------|
| `model` | Yes | Resolved to a Claude model ID |
| `messages` | Yes | Converted to a prompt + optional system prompt |
| `stream` | Yes | Toggles SSE streaming vs JSON response |
| `max_tokens` | Yes | Forwarded via `--max-tokens` |
| `thread_id` | Yes | Maps to `--resume` for multi-turn sessions |
| `temperature` | No | Accepted for compatibility, ignored |
| `tools` | No | Accepted for compatibility, ignored |
| `tool_choice` | No | Accepted for compatibility, ignored |

**Non-streaming response:**

```json
{
  "id": "chatcmpl-<uuid>",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "claude-sonnet-4",
  "choices": [{
    "index": 0,
    "message": { "role": "assistant", "content": "Hello!" },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 5,
    "total_tokens": 15
  }
}
```

**Streaming response** uses Server-Sent Events (SSE), with each `data:` line containing a `chat.completion.chunk` object, terminated by `data: [DONE]`.

## Multi-turn conversations

Include a `thread_id` in your request to continue a conversation. The proxy maintains a session map that translates thread IDs to Claude CLI session IDs, enabling `--resume` across requests.

```json
{ "model": "sonnet", "messages": [...], "thread_id": "my-thread-1" }
```

Sessions are cleaned up automatically after a TTL (10x the configured timeout).

## Tool calls

When Claude uses tools (e.g. file reads, shell commands), the proxy translates them into OpenAI-format tool calls in the response:

```json
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": null,
      "tool_calls": [{
        "id": "toolu_01abc",
        "type": "function",
        "function": { "name": "Bash", "arguments": "{\"command\":\"ls\"}" }
      }]
    },
    "finish_reason": "tool_calls"
  }]
}
```

## Architecture

```
src/
  main.rs              Entry point, CLI parsing, banner
  lib.rs               Public module exports
  config.rs            CLI argument definitions (clap)
  server.rs            Axum router, CORS, graceful shutdown
  session.rs           Thread-to-session mapping with TTL
  error.rs             Error types -> OpenAI error JSON
  handlers/
    health.rs          GET /health
    models.rs          GET /v1/models
    chat.rs            POST /v1/chat/completions (streaming + non-streaming)
  cli/
    types.rs           Claude CLI NDJSON event types
    verify.rs          CLI binary verification
    subprocess.rs      Spawn + parse claude subprocess
  adapter/
    model_map.rs       Model name resolution
    request.rs         OpenAI messages -> CLI prompt conversion
    response.rs        CLI output -> OpenAI response conversion
  types/
    openai.rs          OpenAI API type definitions
tests/
  handler_tests.rs     Integration tests for HTTP endpoints
```

## Development

```bash
# Run tests
cargo test

# Run with verbose logging
RUST_LOG=debug cargo run -- --verbose

# Build release
cargo build --release
```

## License

See repository for license details.

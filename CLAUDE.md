# CLAUDE.md — Project Guide for Claude Code

## What this project is

`claude-code-rusty-proxy` is a Rust HTTP server that exposes an **OpenAI-compatible chat completions API** backed by the **Claude CLI** (`claude --print`). It translates OpenAI requests into CLI subprocess invocations and converts the NDJSON stream back into OpenAI-shaped responses (including SSE streaming).

## Quick start

```bash
cargo build --release
./target/release/claude-code-rusty-proxy          # defaults: 127.0.0.1:3456, sonnet
./target/release/claude-code-rusty-proxy --verbose # debug logging
```

Requires `claude` on `$PATH` and authenticated.

## Project layout

```
src/
  main.rs              — CLI parsing, tracing init, startup banner
  lib.rs               — public re-exports for integration tests
  config.rs            — clap-derived Config struct (port, host, timeout, model, verbose)
  server.rs            — Axum router, CORS, graceful shutdown, session cleanup task
  session.rs           — thread_id → session_id mapping with TTL (for --resume)
  error.rs             — ProxyError enum → OpenAI error JSON responses
  handlers/
    health.rs          — GET /health
    models.rs          — GET /v1/models
    chat.rs            — POST /v1/chat/completions (streaming + non-streaming)
  cli/
    types.rs           — serde types for Claude CLI NDJSON events
    verify.rs          — claude --version check at startup
    subprocess.rs      — spawn claude, parse NDJSON, aggregate output
  adapter/
    model_map.rs       — flexible model name resolution (sonnet/opus/haiku aliases)
    request.rs         — OpenAI messages → CLI prompt + system prompt
    response.rs        — CLI output → OpenAI ChatCompletion / SSE chunks
  types/
    openai.rs          — OpenAI wire types (request, response, chunks, models)
tests/
  handler_tests.rs     — in-process HTTP integration tests (no real CLI needed)
```

## Key architectural patterns

- **Subprocess-per-request**: Each `/v1/chat/completions` call spawns a `claude --print` process. Streaming requests pipe stdout into SSE; non-streaming requests collect full output.
- **Session resume**: The `thread_id` request field maps to Claude CLI's `--resume <session_id>`. Sessions are stored in-memory with a configurable TTL (10× timeout).
- **Model name flexibility**: `resolve_model()` accepts `"sonnet"`, `"claude-sonnet-4"`, `"claude-sonnet-4-20250514"`, etc. Unrecognised names fall back to the configured default.
- **OpenAI error shape**: All errors produce `{ "error": { "message", "type", "param", "code" } }` with appropriate HTTP status codes.

## Running tests

```bash
cargo test              # 97 unit + integration tests, no claude binary needed
cargo test -- --nocapture  # with output
```

Tests are structured as:
- **Unit tests** (inline `#[cfg(test)]` modules in each source file) — cover serde, model resolution, message conversion, response building, session management, error mapping.
- **Integration tests** (`tests/handler_tests.rs`) — drive the full Axum router via `tower::ServiceExt::oneshot` to validate HTTP endpoints.

## Common operations

| Task | Command |
|------|---------|
| Build debug | `cargo build` |
| Build release | `cargo build --release` |
| Run tests | `cargo test` |
| Run with debug logs | `RUST_LOG=debug cargo run -- --verbose` |
| Check lints | `cargo clippy` |
| Format code | `cargo fmt` |

## Configuration

All options support both CLI flags and environment variables:

| Flag | Env var | Default | Description |
|------|---------|---------|-------------|
| `--port` | `PROXY_PORT` | `3456` | TCP port |
| `--host` | `PROXY_HOST` | `127.0.0.1` | Bind address |
| `--timeout` | `PROXY_TIMEOUT` | `300` | Request timeout (seconds) |
| `--default-model` | `PROXY_DEFAULT_MODEL` | `sonnet` | Fallback model |
| `--verbose` | `PROXY_VERBOSE` | `false` | Debug logging |

## Important implementation notes

- The `CLAUDE_CODE` env var is removed before spawning the CLI subprocess to prevent recursion when the proxy itself runs inside Claude Code.
- `--max-turns 1` is always passed to limit the CLI to a single agentic turn per request.
- Mutex poisoning is handled with `unwrap_or_else(|p| p.into_inner())` for resilience.
- The SSE stream sends a 15-second keep-alive to prevent proxy/load-balancer timeouts.

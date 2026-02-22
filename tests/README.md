# tests/

Integration tests for the HTTP API layer.

## Running

```bash
cargo test                          # all tests (unit + integration)
cargo test --test handler_tests     # integration tests only
cargo test -- --nocapture           # with stdout/stderr output
```

## Test files

### [`handler_tests.rs`](handler_tests.rs)

Drives the full Axum router in-process using `tower::ServiceExt::oneshot` — no TCP socket, no real `claude` binary needed.

**Tests:**

| Test | What it validates |
|------|-------------------|
| `test_health_endpoint` | `GET /health` returns 200 with `{"status":"ok","provider":"claude-code-rusty-proxy"}` |
| `test_models_endpoint` | `GET /v1/models` returns all 3 models (opus, sonnet, haiku) with correct metadata (`object:"model"`, `owned_by:"anthropic"`) |
| `test_chat_completions_empty_messages` | Empty `messages: []` returns 400 with `invalid_request_error` and message containing "empty" |
| `test_chat_completions_invalid_json` | Non-JSON body returns 400 or 422 (Axum deserialization rejection) |
| `test_chat_completions_system_only_messages` | System-only messages (no user content) returns 400 with `invalid_request_error` and "No user content" message |

**Helpers:**

- `test_config()` — builds a `Config` with port 0, timeout 300, default model "sonnet".
- `build_app()` — constructs the router with test state.
- `assert_openai_error()` — validates the OpenAI error JSON envelope structure.

## Unit tests

Unit tests live alongside the code they test in `#[cfg(test)]` modules within each source file. See each module's doc comments for details on what each test validates.

**Test counts by module:**

| Module | Tests | Focus |
|--------|-------|-------|
| `types::openai` | 16 | Request/response serde, content variants, round-trips |
| `cli::types` | 14 | NDJSON event parsing for all event types |
| `cli::subprocess` | 12 | NDJSON stream aggregation, usage summing, edge cases |
| `adapter::model_map` | 7 | Name resolution, aliases, fallback, recursion |
| `adapter::request` | 10 | Message conversion for all roles and edge cases |
| `adapter::response` | 12 | Response/chunk building, tool calls, usage mapping |
| `session` | 9 | Store/get, TTL, cleanup, concurrent access |
| `error` | 7 | Status codes, error types, JSON shape |
| `config` | 2 | Default and custom CLI argument parsing |

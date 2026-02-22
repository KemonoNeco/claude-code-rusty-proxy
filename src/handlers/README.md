# handlers/

HTTP request handlers for all public API endpoints.

## Endpoints

### `GET /health` — [`health.rs`](health.rs)

Returns a static JSON health check response. No dependencies, no state.

```json
{ "status": "ok", "provider": "claude-code-rusty-proxy" }
```

### `GET /v1/models` — [`models.rs`](models.rs)

Lists all available Claude models in the OpenAI `ModelList` format.
Models are sourced from [`adapter::model_map::available_models()`](../adapter/model_map.rs).

### `POST /v1/chat/completions` — [`chat.rs`](chat.rs)

The core endpoint. Handles both streaming (SSE) and non-streaming modes.

**Request flow:**

1. Validate the request (non-empty messages, user content present).
2. Resolve the model name via `adapter::model_map`.
3. Convert the OpenAI messages array into a CLI prompt via `adapter::request`.
4. Look up an existing session for `--resume` support.
5. Dispatch to either:
   - **Non-streaming**: spawn `claude --print`, collect full output, return JSON.
   - **Streaming**: spawn `claude --print`, pipe stdout into an SSE stream.

**Shared state** (`AppState`) contains:
- `config` — server configuration
- `session_manager` — thread-to-session mapping

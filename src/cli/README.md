# cli/

Claude CLI integration layer — binary verification, subprocess management, and NDJSON parsing.

## Modules

### [`verify.rs`](verify.rs)

Runs `claude --version` at startup to check the CLI is installed. Returns the version string on success. Non-fatal — the server starts even if verification fails.

### [`subprocess.rs`](subprocess.rs)

Spawns `claude --print` as a child process and parses its output.

**Key types:**
- `CliArgs` — borrowed arguments for building the command (prompt, system prompt, model, session ID, max tokens).
- `CliOutput` — accumulated result: session ID, text content, tool calls, usage tokens, error flag.
- `CliToolCall` — a single tool invocation (id, name, arguments JSON).

**Key functions:**
- `run_claude()` — non-streaming: spawns the CLI, collects all NDJSON events, applies timeout, returns `CliOutput`.
- `spawn_claude_streaming()` — streaming: spawns the CLI and returns the child + stdout pipe for the caller to read line-by-line.
- `parse_event()` / `process_event()` — stateless event parsing and stateful accumulation.

**CLI flags always set:**
- `--output-format stream-json` — NDJSON on stdout
- `--verbose` — includes usage information in events
- `--max-turns 1` — single agentic turn per request
- `CLAUDE_CODE` env var removed to prevent recursion

### [`types.rs`](types.rs)

Serde types for the Claude CLI's `--output-format stream-json` NDJSON events.

**Event types emitted by the CLI:**

| `type` | Description |
|--------|-------------|
| `system` | Session init — carries `session_id` |
| `assistant` | LLM response — `message.content[]` with `text` and `tool_use` blocks |
| `user` | Tool results — `message.content[]` with `tool_result` blocks |
| `result` | Final summary — `is_error`, `duration_ms`, `num_turns`, result text |

Usage information (`input_tokens`, `output_tokens`) can appear at both the event level and inside `message.usage`.

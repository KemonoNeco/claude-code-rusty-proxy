//! Claude CLI integration layer.
//!
//! * [`verify`]     – check that the `claude` binary is installed.
//! * [`subprocess`] – spawn the CLI, parse its NDJSON output.
//! * [`types`]      – Serde types for the CLI's `--output-format stream-json` events.

pub mod subprocess;
pub mod types;
pub mod verify;

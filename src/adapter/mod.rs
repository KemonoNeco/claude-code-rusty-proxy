//! Adapters that translate between the OpenAI wire format and the Claude CLI.
//!
//! * [`model_map`] – resolve flexible model name strings to concrete model IDs.
//! * [`request`]   – convert an OpenAI messages array into a CLI prompt.
//! * [`response`]  – convert CLI output into OpenAI response / SSE chunk objects.

pub mod model_map;
pub mod request;
pub mod response;

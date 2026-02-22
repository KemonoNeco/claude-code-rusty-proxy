//! Flexible model name resolution.
//!
//! Accepts short aliases (`sonnet`), display names (`claude-sonnet-4`), and
//! full dated IDs (`claude-sonnet-4-20250514`). Unrecognised names fall back
//! to the configured default, which itself recurses with `"sonnet"` as the
//! ultimate fallback to guarantee termination.

/// A resolved Claude model with both the API model ID and a shorter
/// human-facing display name.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeModel {
    /// Full model identifier passed to `--model` (e.g. `claude-sonnet-4-20250514`).
    pub id: &'static str,
    /// Short display name returned in API responses (e.g. `claude-sonnet-4`).
    pub display_name: &'static str,
}

pub const OPUS: ClaudeModel = ClaudeModel {
    id: "claude-opus-4-20250514",
    display_name: "claude-opus-4",
};

pub const SONNET: ClaudeModel = ClaudeModel {
    id: "claude-sonnet-4-20250514",
    display_name: "claude-sonnet-4",
};

pub const HAIKU: ClaudeModel = ClaudeModel {
    id: "claude-haiku-4-20250506",
    display_name: "claude-haiku-4",
};

/// Resolve an arbitrary model name to a [`ClaudeModel`].
///
/// Matching is case-insensitive and whitespace-trimmed. If `input` does not
/// match any known model, the function recurses with `default` as the input
/// and `"sonnet"` as the fallback, guaranteeing a valid return value.
pub fn resolve_model(input: &str, default: &str) -> ClaudeModel {
    let normalized = input.to_lowercase();
    let normalized = normalized.trim();

    // Exact or prefix matches for opus
    if normalized == "opus"
        || normalized.contains("opus-4")
        || normalized.contains("claude-opus")
        || normalized == "claude-opus-4-20250514"
    {
        return OPUS;
    }

    // Exact or prefix matches for haiku
    if normalized == "haiku"
        || normalized.contains("haiku-4")
        || normalized.contains("claude-haiku")
        || normalized == "claude-haiku-4-20250506"
    {
        return HAIKU;
    }

    // Exact or prefix matches for sonnet
    if normalized == "sonnet"
        || normalized.contains("sonnet-4")
        || normalized.contains("claude-sonnet")
        || normalized == "claude-sonnet-4-20250514"
    {
        return SONNET;
    }

    // Default fallback
    resolve_model(default, "sonnet")
}

/// Get all available models.
pub fn available_models() -> Vec<ClaudeModel> {
    vec![OPUS, SONNET, HAIKU]
}

#[cfg(test)]
mod tests {
    //! Tests for model name resolution: short aliases, full names, case
    //! insensitivity, whitespace trimming, unknown fallback, and recursive
    //! default termination.

    use super::*;

    /// All recognised sonnet aliases resolve to `SONNET`.
    #[test]
    fn test_resolve_sonnet_variants() {
        assert_eq!(resolve_model("sonnet", "sonnet"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4", "sonnet"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4-20250514", "sonnet"), SONNET);
        assert_eq!(resolve_model("SONNET", "sonnet"), SONNET);
        assert_eq!(resolve_model("Claude-Sonnet-4", "sonnet"), SONNET);
    }

    /// All recognised opus aliases resolve to `OPUS`.
    #[test]
    fn test_resolve_opus_variants() {
        assert_eq!(resolve_model("opus", "sonnet"), OPUS);
        assert_eq!(resolve_model("claude-opus-4", "sonnet"), OPUS);
        assert_eq!(resolve_model("claude-opus-4-20250514", "sonnet"), OPUS);
        assert_eq!(resolve_model("OPUS", "sonnet"), OPUS);
    }

    /// All recognised haiku aliases resolve to `HAIKU`.
    #[test]
    fn test_resolve_haiku_variants() {
        assert_eq!(resolve_model("haiku", "sonnet"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4", "sonnet"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4-20250506", "sonnet"), HAIKU);
        assert_eq!(resolve_model("HAIKU", "sonnet"), HAIKU);
    }

    /// Unrecognised model names fall back to the configured default.
    #[test]
    fn test_unknown_defaults_to_configured() {
        assert_eq!(resolve_model("gpt-4o", "sonnet"), SONNET);
        assert_eq!(resolve_model("gpt-4o", "opus"), OPUS);
        assert_eq!(resolve_model("unknown-model", "haiku"), HAIKU);
    }

    /// Leading/trailing whitespace is trimmed before matching.
    #[test]
    fn test_whitespace_handling() {
        assert_eq!(resolve_model("  sonnet  ", "sonnet"), SONNET);
        assert_eq!(resolve_model("  opus  ", "sonnet"), OPUS);
    }

    /// `available_models()` returns exactly opus, sonnet, and haiku.
    #[test]
    fn test_available_models() {
        let models = available_models();
        assert_eq!(models.len(), 3);
        assert!(models.contains(&OPUS));
        assert!(models.contains(&SONNET));
        assert!(models.contains(&HAIKU));
    }

    /// When both input and default are unknown, recursion terminates at `"sonnet"`.
    #[test]
    fn test_resolve_model_recursive_termination() {
        // When both input and default are unrecognized, the recursive call uses
        // "sonnet" as the fallback default, which must resolve.
        let result = resolve_model("totally-unknown", "also-unknown");
        assert_eq!(result, SONNET);
    }
}

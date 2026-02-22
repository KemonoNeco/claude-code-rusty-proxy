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
    id: "claude-opus-4-6",
    display_name: "claude-opus-4-6",
};

pub const SONNET: ClaudeModel = ClaudeModel {
    id: "claude-sonnet-4-6",
    display_name: "claude-sonnet-4-6",
};

pub const HAIKU: ClaudeModel = ClaudeModel {
    id: "claude-haiku-4-5-20251001",
    display_name: "claude-haiku-4-5",
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
        || normalized.starts_with("opus-")
        || normalized.starts_with("claude-opus")
    {
        return OPUS;
    }

    // Exact or prefix matches for haiku (checked before sonnet)
    if normalized == "haiku"
        || normalized.starts_with("haiku-")
        || normalized.starts_with("claude-haiku")
    {
        return HAIKU;
    }

    // Exact or prefix matches for sonnet
    if normalized == "sonnet"
        || normalized.starts_with("sonnet-")
        || normalized.starts_with("claude-sonnet")
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
    //! insensitivity, whitespace trimming, unknown fallback, recursive
    //! default termination, and adversarial inputs.

    use super::*;

    // ── Happy-path tests ────────────────────────────────────────────

    /// All recognised sonnet aliases resolve to `SONNET`.
    /// Uses a non-sonnet default to ensure the match is direct, not via fallback.
    #[test]
    fn test_resolve_sonnet_variants() {
        assert_eq!(resolve_model("sonnet", "opus"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4", "opus"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4-6", "opus"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4-20250514", "opus"), SONNET);
        assert_eq!(resolve_model("SONNET", "opus"), SONNET);
        assert_eq!(resolve_model("Claude-Sonnet-4", "opus"), SONNET);
        assert_eq!(resolve_model("sonnet-4", "opus"), SONNET);
        assert_eq!(resolve_model("sonnet-4-6", "opus"), SONNET);
    }

    /// All recognised opus aliases resolve to `OPUS`.
    /// Uses a non-opus default to ensure the match is direct, not via fallback.
    #[test]
    fn test_resolve_opus_variants() {
        assert_eq!(resolve_model("opus", "haiku"), OPUS);
        assert_eq!(resolve_model("claude-opus-4", "haiku"), OPUS);
        assert_eq!(resolve_model("claude-opus-4-6", "haiku"), OPUS);
        assert_eq!(resolve_model("claude-opus-4-20250514", "haiku"), OPUS);
        assert_eq!(resolve_model("OPUS", "haiku"), OPUS);
        assert_eq!(resolve_model("opus-4", "haiku"), OPUS);
        assert_eq!(resolve_model("opus-4-6", "haiku"), OPUS);
    }

    /// All recognised haiku aliases resolve to `HAIKU`.
    /// Uses a non-haiku default to ensure the match is direct, not via fallback.
    #[test]
    fn test_resolve_haiku_variants() {
        assert_eq!(resolve_model("haiku", "opus"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4", "opus"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4-5", "opus"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4-5-20251001", "opus"), HAIKU);
        assert_eq!(resolve_model("HAIKU", "opus"), HAIKU);
        assert_eq!(resolve_model("haiku-4", "opus"), HAIKU);
        assert_eq!(resolve_model("haiku-4-5", "opus"), HAIKU);
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
        let result = resolve_model("totally-unknown", "also-unknown");
        assert_eq!(result, SONNET);
    }

    // ── Adversarial tests — .contains() false-positive prevention ───

    /// Substring "opus-4" embedded in a non-opus name must NOT match OPUS.
    #[test]
    fn test_adversarial_contains_false_positive_opus() {
        assert_eq!(resolve_model("not-opus-4-at-all", "sonnet"), SONNET);
    }

    /// Substring "sonnet-4" embedded in a non-sonnet name must NOT match SONNET.
    #[test]
    fn test_adversarial_contains_false_positive_sonnet() {
        assert_eq!(resolve_model("my-custom-sonnet-4-fork", "opus"), OPUS);
    }

    /// Substring "haiku-4" embedded in a non-haiku name must NOT match HAIKU.
    #[test]
    fn test_adversarial_contains_false_positive_haiku() {
        assert_eq!(resolve_model("not-haiku-4-model", "sonnet"), SONNET);
    }

    /// "claude-opus" as a substring in a longer unrelated name must NOT match.
    #[test]
    fn test_adversarial_substring_in_unrelated_word() {
        // "anthropic-claude-opus-experiment" starts with "anthropic-", not "claude-opus"
        assert_eq!(
            resolve_model("anthropic-claude-opus-experiment", "sonnet"),
            SONNET
        );
    }

    /// A name wrapping a valid model name with extra prefix must NOT match.
    #[test]
    fn test_adversarial_model_name_with_prefix() {
        assert_eq!(
            resolve_model("pre-claude-sonnet-4-suffix", "opus"),
            OPUS
        );
    }

    /// When input contains both "opus-" and "sonnet-" as substrings,
    /// starts_with matches the first one (opus) due to check order.
    #[test]
    fn test_adversarial_combined_substrings() {
        // "opus-4-sonnet-4" starts with "opus-" → OPUS wins
        assert_eq!(resolve_model("opus-4-sonnet-4", "sonnet"), OPUS);
    }

    // ── Model constant freshness tests ──────────────────────────────

    /// Model constants must reflect current Anthropic model IDs.
    #[test]
    fn test_model_constants_reflect_current_versions() {
        assert_eq!(OPUS.id, "claude-opus-4-6");
        assert_eq!(OPUS.display_name, "claude-opus-4-6");
        assert_eq!(SONNET.id, "claude-sonnet-4-6");
        assert_eq!(SONNET.display_name, "claude-sonnet-4-6");
        assert_eq!(HAIKU.id, "claude-haiku-4-5-20251001");
        assert_eq!(HAIKU.display_name, "claude-haiku-4-5");
    }

    /// Resolving the new 4.6 model versions must return the correct model.
    #[test]
    fn test_resolve_new_model_versions() {
        let opus = resolve_model("claude-opus-4-6", "sonnet");
        assert_eq!(opus.id, "claude-opus-4-6");

        let sonnet = resolve_model("claude-sonnet-4-6", "sonnet");
        assert_eq!(sonnet.id, "claude-sonnet-4-6");
    }

    // ── Robustness tests — weird/malicious inputs ───────────────────

    /// Empty string falls back to default, no panic.
    #[test]
    fn test_empty_string_falls_back() {
        assert_eq!(resolve_model("", "sonnet"), SONNET);
        assert_eq!(resolve_model("", "opus"), OPUS);
    }

    /// Whitespace-only input falls back to default.
    #[test]
    fn test_whitespace_only_falls_back() {
        assert_eq!(resolve_model("   \t\n  ", "sonnet"), SONNET);
    }

    /// Null characters in model name don't cause panics.
    #[test]
    fn test_null_characters_in_model_name() {
        let result = resolve_model("sonnet\0evil", "sonnet");
        // Contains a null byte, so won't exactly match any alias —
        // but starts_with("sonnet-") is false because next char is \0 not '-'.
        // Falls back to default.
        assert_eq!(result, SONNET);
    }

    /// Unicode (zero-width space prefix) falls back to default.
    #[test]
    fn test_unicode_model_name() {
        let input = "\u{200B}sonnet"; // zero-width space + "sonnet"
        let result = resolve_model(input, "opus");
        // Won't match "sonnet" exactly due to the invisible prefix
        assert_eq!(result, OPUS);
    }

    /// Very long model name (100K chars) doesn't hang or panic.
    #[test]
    fn test_very_long_model_name() {
        let long_name = "x".repeat(100_000);
        let result = resolve_model(&long_name, "sonnet");
        assert_eq!(result, SONNET);
    }
}

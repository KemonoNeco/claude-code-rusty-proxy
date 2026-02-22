//! Model name resolution for flexible matching.

/// Resolved Claude model identifier.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeModel {
    pub id: &'static str,
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

/// Resolve a model name string to a Claude model.
///
/// Supports full names, short aliases, and prefixed variations.
/// Returns the default model if unrecognized.
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
    use super::*;

    #[test]
    fn test_resolve_sonnet_variants() {
        assert_eq!(resolve_model("sonnet", "sonnet"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4", "sonnet"), SONNET);
        assert_eq!(resolve_model("claude-sonnet-4-20250514", "sonnet"), SONNET);
        assert_eq!(resolve_model("SONNET", "sonnet"), SONNET);
        assert_eq!(resolve_model("Claude-Sonnet-4", "sonnet"), SONNET);
    }

    #[test]
    fn test_resolve_opus_variants() {
        assert_eq!(resolve_model("opus", "sonnet"), OPUS);
        assert_eq!(resolve_model("claude-opus-4", "sonnet"), OPUS);
        assert_eq!(resolve_model("claude-opus-4-20250514", "sonnet"), OPUS);
        assert_eq!(resolve_model("OPUS", "sonnet"), OPUS);
    }

    #[test]
    fn test_resolve_haiku_variants() {
        assert_eq!(resolve_model("haiku", "sonnet"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4", "sonnet"), HAIKU);
        assert_eq!(resolve_model("claude-haiku-4-20250506", "sonnet"), HAIKU);
        assert_eq!(resolve_model("HAIKU", "sonnet"), HAIKU);
    }

    #[test]
    fn test_unknown_defaults_to_configured() {
        assert_eq!(resolve_model("gpt-4o", "sonnet"), SONNET);
        assert_eq!(resolve_model("gpt-4o", "opus"), OPUS);
        assert_eq!(resolve_model("unknown-model", "haiku"), HAIKU);
    }

    #[test]
    fn test_whitespace_handling() {
        assert_eq!(resolve_model("  sonnet  ", "sonnet"), SONNET);
        assert_eq!(resolve_model("  opus  ", "sonnet"), OPUS);
    }

    #[test]
    fn test_available_models() {
        let models = available_models();
        assert_eq!(models.len(), 3);
        assert!(models.contains(&OPUS));
        assert!(models.contains(&SONNET));
        assert!(models.contains(&HAIKU));
    }

    #[test]
    fn test_resolve_model_recursive_termination() {
        // When both input and default are unrecognized, the recursive call uses
        // "sonnet" as the fallback default, which must resolve.
        let result = resolve_model("totally-unknown", "also-unknown");
        assert_eq!(result, SONNET);
    }
}

pub const DEFAULT_CONTEXT_LIMIT: usize = 32_000;

pub fn model_context_limit(provider: &str, model: &str) -> usize {
    let model = model.to_ascii_lowercase();
    if provider.eq_ignore_ascii_case("gemini") {
        if model.contains("gemini-1.5") || model.contains("gemini-2") || model.contains("gemini-3") {
            return 1_000_000;
        }
        return DEFAULT_CONTEXT_LIMIT;
    }

    if model.contains("gpt-4o") || model.contains("gpt-4.1") {
        128_000
    } else {
        DEFAULT_CONTEXT_LIMIT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_gemini_models_have_large_context_and_unknown_models_are_bounded() {
        assert_eq!(model_context_limit("gemini", "gemini-2.5-flash"), 1_000_000);
        assert_eq!(model_context_limit("gemini", "custom-model"), DEFAULT_CONTEXT_LIMIT);
    }
}

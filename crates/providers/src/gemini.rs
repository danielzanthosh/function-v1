use crate::{
    context_limits::model_context_limit, CompletionRequest, CompletionResponse, LlmProvider,
    OpenAiLlmProvider, ProviderError,
};
use async_trait::async_trait;

pub const GEMINI_OPENAI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/";

pub struct GeminiLlmProvider {
    inner: OpenAiLlmProvider,
    default_model: String,
}

impl GeminiLlmProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_model: impl Into<String>,
    ) -> Self {
        let default_model = default_model.into();
        Self {
            inner: OpenAiLlmProvider::new(base_url, api_key, default_model.clone()),
            default_model,
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiLlmProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn context_limit(&self, model: &str) -> usize {
        model_context_limit(
            "gemini",
            if model.is_empty() || model == "default" {
                &self.default_model
            } else {
                model
            },
        )
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.inner.complete(req).await
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        on_token: Box<dyn FnMut(String) + Send>,
    ) -> Result<CompletionResponse, ProviderError> {
        self.inner.complete_stream(req, on_token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_provider_uses_google_defaults_and_context_limits() {
        let provider = GeminiLlmProvider::new(
            GEMINI_OPENAI_BASE_URL,
            Some("test-key".to_string()),
            "gemini-2.5-flash",
        );
        assert_eq!(provider.name(), "gemini");
        assert_eq!(provider.context_limit(""), 1_000_000);
    }
}

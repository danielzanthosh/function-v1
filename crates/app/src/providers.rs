use std::sync::Arc;

use function_config::AppConfig;
use function_providers::{
    CodexChatGptProvider, GeminiLlmProvider, LlmProvider, MockLlmProvider, MockSttProvider, OpenAiLlmProvider,
    OpenAiTtsProvider, SpeechToTextProvider, TextToSpeechProvider, WhisperSttProvider,
};

fn ai_base_url(config: &AppConfig) -> String {
    if config.ai_provider.provider_name.eq_ignore_ascii_case("gemini")
        && (config.ai_provider.base_url.trim().is_empty()
            || config.ai_provider.base_url.trim() == "https://api.openai.com/v1")
    {
        function_providers::gemini::GEMINI_OPENAI_BASE_URL.to_string()
    } else {
        config.ai_provider.base_url.clone()
    }
}

pub fn build_llm_provider(config: &AppConfig) -> Arc<dyn LlmProvider> {
    if config.ai_provider.provider_name.eq_ignore_ascii_case("chatgpt-plan") {
        return Arc::new(CodexChatGptProvider::new(&config.ai_provider.model));
    }
    let api_key = config.ai_provider.resolve_api_key(&function_config::InMemoryCredentialStore::new());
    if !config.ai_provider.is_configured() {
        return Arc::new(MockLlmProvider::new(
            "Function computer assistant ready. Configure your API key in settings or run computer tools directly.",
        ));
    }

    if config.ai_provider.provider_name.eq_ignore_ascii_case("gemini") {
        Arc::new(GeminiLlmProvider::new(
            ai_base_url(config),
            api_key,
            &config.ai_provider.model,
        ))
    } else {
        Arc::new(OpenAiLlmProvider::new(
            &config.ai_provider.base_url,
            api_key,
            &config.ai_provider.model,
        ))
    }
}

pub fn build_stt_provider(config: &AppConfig) -> Arc<dyn SpeechToTextProvider> {
    if config.speech.is_configured() {
        Arc::new(WhisperSttProvider::with_model(
            &config.speech.base_url,
            config.speech.resolve_api_key(),
            &config.speech.model,
        ))
    } else {
        Arc::new(MockSttProvider::new("Open my browser and navigate to YouTube"))
    }
}

pub fn build_tts_provider(config: &AppConfig) -> Option<Arc<dyn TextToSpeechProvider>> {
    if !config.tts.enabled {
        return None;
    }
    config.tts.resolve_api_key().map(|api_key| {
        Arc::new(OpenAiTtsProvider::new(
            &config.tts.base_url,
            Some(api_key),
            &config.tts.model,
            &config.tts.voice,
            &config.tts.output_format,
        )) as Arc<dyn TextToSpeechProvider>
    })
}

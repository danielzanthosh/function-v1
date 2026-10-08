//! External provider abstractions for LLM, speech-to-text, text-to-speech, and web search.
//!
//! Provides a vendor-neutral interface with support for OpenAI-compatible LLM endpoints,
//! Whisper-based speech recognition, and pluggable search services.

pub mod chatgpt_oauth;
pub mod codex;
pub mod context_limits;
pub mod gemini;
pub mod image_util;
pub mod openai;
pub mod retry;
pub mod search;

pub use codex::CodexChatGptProvider;
pub use gemini::GeminiLlmProvider;
pub use openai::{OpenAiLlmProvider, OpenAiTtsProvider, WhisperSttProvider};
pub use retry::{RetryingLlmProvider, StatusCallback};
pub use search::DuckDuckGoSearchProvider;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error ({code}): {message}")]
    Api { code: u16, message: String },
    #[error("Rate limit error ({code}): {message}")]
    RateLimit {
        code: u16,
        message: String,
        retry_after: Option<Duration>,
    },
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Provider not configured: {0}")]
    NotConfigured(String),
}

impl ProviderError {
    pub fn is_rate_limit(&self) -> bool {
        match self {
            ProviderError::RateLimit { .. } => true,
            ProviderError::Api { code, message } => {
                *code == 429 || is_rate_limit_message(message)
            }
            _ => false,
        }
    }

    pub fn is_permanent(&self) -> bool {
        match self {
            ProviderError::Authentication(_) | ProviderError::NotConfigured(_) => true,
            ProviderError::Api { code, message } => {
                if *code == 429 || is_rate_limit_message(message) {
                    return false;
                }
                if *code == 401 || *code == 403 || *code == 400 || *code == 404 {
                    return true;
                }
                let lower = message.to_lowercase();
                lower.contains("invalid api key")
                    || lower.contains("invalid_api_key")
                    || lower.contains("unauthorized")
                    || lower.contains("model_not_found")
                    || lower.contains("invalid model")
            }
            ProviderError::RateLimit { .. } => false,
            _ => false,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            ProviderError::RateLimit { retry_after, .. } => *retry_after,
            ProviderError::Api { message, .. } => parse_retry_delay_from_message(message),
            _ => None,
        }
    }
}

pub fn is_rate_limit_message(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("resource_exhausted")
        || lower.contains("resource exhausted")
        || lower.contains("too many requests")
        || lower.contains("tokens per minute")
        || lower.contains("tpm")
        || lower.contains("requests per minute")
        || lower.contains("rpm")
        || lower.contains("quota exceeded")
        || lower.contains("retry after")
}

pub fn parse_retry_delay_from_message(msg: &str) -> Option<Duration> {
    let lower = msg.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    for i in 0..words.len() {
        let clean_word = words[i].trim_matches(|c: char| !c.is_alphanumeric());
        if clean_word == "in" || clean_word == "after" || clean_word == "wait" {
            if i + 1 < words.len() {
                let next = words[i + 1].trim_matches(|c: char| !c.is_alphanumeric());
                if let Some(dur) = parse_time_str(next) {
                    return Some(dur);
                }
                if i + 2 < words.len() {
                    let unit_part = words[i + 2].trim_matches(|c: char| !c.is_alphanumeric());
                    if let Ok(val) = next.parse::<f64>() {
                        if unit_part.starts_with("sec") || unit_part == "s" {
                            return Some(Duration::from_secs_f64(val.max(0.1)));
                        } else if unit_part.starts_with("ms") || unit_part.starts_with("milli") {
                            return Some(Duration::from_millis((val.max(1.0)) as u64));
                        } else if unit_part.starts_with("min") || unit_part == "m" {
                            return Some(Duration::from_secs_f64((val * 60.0).max(1.0)));
                        }
                    }
                }
            }
        }
        if let Some(dur) = parse_time_str(clean_word) {
            return Some(dur);
        }
    }
    None
}

fn parse_time_str(s: &str) -> Option<Duration> {
    let clean = s.trim_matches(|c: char| !c.is_alphanumeric());
    if clean.ends_with("ms") {
        let num = clean.trim_end_matches("ms").parse::<f64>().ok()?;
        Some(Duration::from_millis(num.max(1.0) as u64))
    } else if clean.ends_with('s') {
        let num = clean.trim_end_matches('s').parse::<f64>().ok()?;
        Some(Duration::from_secs_f64(num.max(0.1)))
    } else {
        None
    }
}

/// Message role for LLM completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Structured chat message exchanged with the AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            images: None,
            tool_call_id: None,
            tool_calls: None,
            thought_signature: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            images: None,
            tool_call_id: None,
            tool_calls: None,
            thought_signature: None,
        }
    }

    pub fn user_with_images(content: impl Into<String>, images: Vec<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            images: Some(images),
            tool_call_id: None,
            tool_calls: None,
            thought_signature: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            images: None,
            tool_call_id: None,
            tool_calls: None,
            thought_signature: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            images: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
            thought_signature: None,
        }
    }
}

/// Tool invocation specification sent by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thought_signature: Option<String>,
}

/// Tool definition presented to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Request sent to the LLM provider.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
}

/// Response returned from the LLM provider.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// Abstraction for OpenAI-compatible LLM providers.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Return the provider identifier name.
    fn name(&self) -> &str;

    /// Return Any reference for downcasting if needed.
    fn as_any(&self) -> &dyn Any;

    /// Context window size in tokens for the selected model.
    fn context_limit(&self, _model: &str) -> usize {
        32_000
    }

    /// Execute a chat completion request.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError>;

    /// Execute a chat completion request with an optional token streaming callback.
    async fn complete_stream(
        &self,
        req: CompletionRequest,
        mut on_token: Box<dyn FnMut(String) + Send>,
    ) -> Result<CompletionResponse, ProviderError> {
        let res = self.complete(req).await?;
        if !res.message.content.is_empty() {
            on_token(res.message.content.clone());
        }
        Ok(res)
    }
}

/// Abstraction for Speech-to-Text providers (e.g. Whisper).
#[async_trait]
pub trait SpeechToTextProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn transcribe_audio(
        &self,
        audio_pcm: &[u8],
        sample_rate: u32,
    ) -> Result<String, ProviderError>;
}

/// Abstraction for Text-to-Speech providers.
#[async_trait]
pub trait TextToSpeechProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn synthesize_speech(&self, text: &str) -> Result<Vec<u8>, ProviderError>;
}

/// Abstraction for optional Web Search providers.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, ProviderError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Mock LLM provider for testing and development.
pub struct MockLlmProvider {
    pub canned_response: String,
}

impl MockLlmProvider {
    pub fn new(canned_response: impl Into<String>) -> Self {
        Self {
            canned_response: canned_response.into(),
        }
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    fn name(&self) -> &str {
        "mock-llm"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        Ok(CompletionResponse {
            message: ChatMessage::assistant(self.canned_response.clone()),
            finish_reason: Some("stop".to_string()),
        })
    }
}

/// Mock Speech-to-Text provider for testing and offline development.
pub struct MockSttProvider {
    pub canned_transcript: String,
}

impl MockSttProvider {
    pub fn new(canned_transcript: impl Into<String>) -> Self {
        Self {
            canned_transcript: canned_transcript.into(),
        }
    }
}

#[async_trait]
impl SpeechToTextProvider for MockSttProvider {
    fn name(&self) -> &str {
        "mock-stt"
    }

    async fn transcribe_audio(
        &self,
        audio_pcm: &[u8],
        _sample_rate: u32,
    ) -> Result<String, ProviderError> {
        if audio_pcm.is_empty() {
            return Ok(String::new());
        }
        Ok(self.canned_transcript.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_retry_delay_from_message() {
        let msg1 = "Please try again in 2.5s.";
        assert_eq!(
            parse_retry_delay_from_message(msg1),
            Some(Duration::from_secs_f64(2.5))
        );

        let msg2 = "RESOURCE_EXHAUSTED: Rate limit exceeded. Retry after 10 seconds.";
        assert_eq!(
            parse_retry_delay_from_message(msg2),
            Some(Duration::from_secs(10))
        );

        let msg3 = "Wait 500ms before retrying";
        assert_eq!(
            parse_retry_delay_from_message(msg3),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn test_provider_error_rate_limit_detection() {
        let err1 = ProviderError::Api {
            code: 429,
            message: "Too many requests".to_string(),
        };
        assert!(err1.is_rate_limit());
        assert!(!err1.is_permanent());

        let err2 = ProviderError::Api {
            code: 401,
            message: "Invalid API key".to_string(),
        };
        assert!(!err2.is_rate_limit());
        assert!(err2.is_permanent());
    }

    #[tokio::test]
    async fn test_mock_llm_provider() {
        let provider = MockLlmProvider::new("Task completed");
        let req = CompletionRequest {
            model: "mock".to_string(),
            messages: vec![ChatMessage::user("Do something")],
            tools: vec![],
            temperature: None,
        };
        let res = provider.complete(req).await.unwrap();
        assert_eq!(res.message.content, "Task completed");
    }

    #[tokio::test]
    async fn test_mock_stt_provider() {
        let provider = MockSttProvider::new("Search the web for Rust documentation");
        let res = provider.transcribe_audio(b"RIFF...", 16000).await.unwrap();
        assert_eq!(res, "Search the web for Rust documentation");
    }
}

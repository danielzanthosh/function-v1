//! External provider abstractions for LLM, speech-to-text, text-to-speech, and web search.
//!
//! Provides a vendor-neutral interface with support for OpenAI-compatible LLM endpoints,
//! Whisper-based speech recognition, and pluggable search services.

pub mod openai;
pub mod search;

pub use openai::{OpenAiLlmProvider, WhisperSttProvider};
pub use search::DuckDuckGoSearchProvider;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error ({code}): {message}")]
    Api { code: u16, message: String },
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Provider not configured: {0}")]
    NotConfigured(String),
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
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// Tool invocation specification sent by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
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

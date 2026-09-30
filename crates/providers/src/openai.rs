//! OpenAI-compatible LLM and Speech-to-Text provider implementations.

use crate::{
    ChatMessage, CompletionRequest, CompletionResponse, LlmProvider, MessageRole, ProviderError,
    SpeechToTextProvider, ToolCall,
};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

pub struct OpenAiLlmProvider {
    base_url: String,
    api_key: Option<String>,
    default_model: String,
}

impl OpenAiLlmProvider {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>, default_model: impl Into<String>) -> Self {
        let mut url = base_url.into();
        if url.ends_with('/') {
            url.pop();
        }
        Self {
            base_url: url,
            api_key,
            default_model: default_model.into(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiLlmProvider {
    fn name(&self) -> &str {
        "openai-compatible"
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let endpoint = format!("{}/chat/completions", self.base_url);
        let model = if req.model.is_empty() {
            &self.default_model
        } else {
            &req.model
        };

        let messages_json: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                let role_str = match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                };

                let mut obj = json!({
                    "role": role_str,
                    "content": m.content,
                });

                if let Some(ref t_id) = m.tool_call_id {
                    obj["tool_call_id"] = json!(t_id);
                }

                if let Some(ref t_calls) = m.tool_calls {
                    let calls_json: Vec<serde_json::Value> = t_calls
                        .iter()
                        .map(|c| {
                            json!({
                                "id": c.id,
                                "type": "function",
                                "function": {
                                    "name": c.name,
                                    "arguments": c.arguments.to_string(),
                                }
                            })
                        })
                        .collect();
                    obj["tool_calls"] = json!(calls_json);
                }

                obj
            })
            .collect();

        let mut payload = json!({
            "model": model,
            "messages": messages_json,
        });

        if let Some(temp) = req.temperature {
            payload["temperature"] = json!(temp);
        }

        if !req.tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = req
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            payload["tools"] = json!(tools_json);
        }

        let body_str = payload.to_string();

        // Perform HTTP request asynchronously via tokio::process or std::process in spawn_blocking
        let base_url = endpoint.clone();
        let api_key = self.api_key.clone();

        let response_body = tokio::task::spawn_blocking(move || -> Result<String, ProviderError> {
            let mut cmd = Command::new("curl.exe");
            cmd.arg("-s")
                .arg("-X")
                .arg("POST")
                .arg(&base_url)
                .arg("-H")
                .arg("Content-Type: application/json");

            if let Some(ref key) = api_key {
                if !key.is_empty() {
                    cmd.arg("-H").arg(format!("Authorization: Bearer {}", key));
                }
            }

            cmd.arg("--data-raw").arg(&body_str);

            let output = cmd.output().map_err(|e| ProviderError::Network(e.to_string()))?;
            if !output.status.success() {
                return Err(ProviderError::Network(format!(
                    "curl exited with code {:?}",
                    output.status.code()
                )));
            }

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        })
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))??;

        let parsed: serde_json::Value = serde_json::from_str(&response_body)?;

        if let Some(err) = parsed.get("error") {
            let msg = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown provider error");
            return Err(ProviderError::Api {
                code: 400,
                message: msg.to_string(),
            });
        }

        let choice = parsed
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| ProviderError::Api {
                code: 500,
                message: "No completion choices returned".into(),
            })?;

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        let msg_val = choice.get("message").ok_or_else(|| ProviderError::Api {
            code: 500,
            message: "Missing message in choice".into(),
        })?;

        let content = msg_val
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let tool_calls = if let Some(t_array) = msg_val.get("tool_calls").and_then(|t| t.as_array()) {
            let mut calls = Vec::new();
            for c in t_array {
                let id = c.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                if let Some(func) = c.get("function") {
                    let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let args_raw = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                    let arguments: serde_json::Value =
                        serde_json::from_str(args_raw).unwrap_or(json!({ "raw": args_raw }));
                    calls.push(ToolCall { id, name, arguments });
                }
            }
            if calls.is_empty() {
                None
            } else {
                Some(calls)
            }
        } else {
            None
        };

        Ok(CompletionResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content,
                tool_call_id: None,
                tool_calls,
            },
            finish_reason,
        })
    }
}

pub struct WhisperSttProvider {
    base_url: String,
    api_key: Option<String>,
}

impl WhisperSttProvider {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let mut url = base_url.into();
        if url.ends_with('/') {
            url.pop();
        }
        Self {
            base_url: url,
            api_key,
        }
    }
}

#[async_trait]
impl SpeechToTextProvider for WhisperSttProvider {
    fn name(&self) -> &str {
        "whisper"
    }

    async fn transcribe_audio(&self, audio_pcm: &[u8], _sample_rate: u32) -> Result<String, ProviderError> {
        if audio_pcm.is_empty() {
            return Ok(String::new());
        }

        let endpoint = format!("{}/audio/transcriptions", self.base_url);
        let api_key = self.api_key.clone();
        let bytes = audio_pcm.to_vec();

        tokio::task::spawn_blocking(move || -> Result<String, ProviderError> {
            let unique_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let temp_file = std::env::temp_dir().join(format!("fn_audio_{}_{}.wav", std::process::id(), unique_id));
            std::fs::write(&temp_file, &bytes)
                .map_err(|e| ProviderError::Network(format!("Failed to write audio: {}", e)))?;

            let curl_bin = if cfg!(target_os = "windows") { "curl.exe" } else { "curl" };
            let mut cmd = Command::new(curl_bin);
            cmd.arg("-s")
                .arg("-X")
                .arg("POST")
                .arg(&endpoint)
                .arg("-F")
                .arg(format!("file=@{}", temp_file.display()))
                .arg("-F")
                .arg("model=whisper-1");

            if let Some(ref key) = api_key {
                if !key.is_empty() {
                    cmd.arg("-H").arg(format!("Authorization: Bearer {}", key));
                }
            }

            let output = cmd.output().map_err(|e| ProviderError::Network(e.to_string()))?;
            let _ = std::fs::remove_file(&temp_file);

            if !output.status.success() {
                return Err(ProviderError::Network(format!(
                    "curl exited with code {:?}",
                    output.status.code()
                )));
            }

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let parsed: serde_json::Value = serde_json::from_str(&stdout)?;

            if let Some(err_obj) = parsed.get("error") {
                let msg = err_obj.get("message").and_then(|m| m.as_str()).unwrap_or("Whisper API error");
                return Err(ProviderError::Api {
                    code: 400,
                    message: msg.to_string(),
                });
            }

            if let Some(text) = parsed.get("text").and_then(|t| t.as_str()) {
                Ok(text.trim().to_string())
            } else {
                Ok(stdout.trim().to_string())
            }
        })
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?
    }
}

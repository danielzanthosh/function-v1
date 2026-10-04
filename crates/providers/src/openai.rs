//! OpenAI-compatible LLM and Speech-to-Text provider implementations.

use crate::{
    ChatMessage, CompletionRequest, CompletionResponse, LlmProvider, MessageRole, ProviderError,
    SpeechToTextProvider, TextToSpeechProvider, ToolCall,
};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

const CURL_STATUS_MARKER: &str = "\n__FUNCTION_HTTP_STATUS__:";

fn provider_error_message(parsed: &serde_json::Value, raw_body: &str) -> String {
    if let Some(error) = parsed.get("error") {
        if let Some(message) = error.get("message").and_then(|value| value.as_str()) {
            return message.to_string();
        }

        if !error.is_null() {
            return error.to_string();
        }
    }

    let fallback = raw_body.trim();
    if fallback.is_empty() {
        "Empty provider response".to_string()
    } else {
        fallback.chars().take(1000).collect()
    }
}

fn split_curl_response(raw_response: String) -> (String, Option<u16>) {
    if let Some((body, status)) = raw_response.rsplit_once(CURL_STATUS_MARKER) {
        return (body.to_string(), status.trim().parse().ok());
    }

    (raw_response, None)
}

pub struct OpenAiLlmProvider {
    base_url: String,
    api_key: Option<String>,
    default_model: String,
}

impl OpenAiLlmProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_model: impl Into<String>,
    ) -> Self {
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

    fn context_limit(&self, model: &str) -> usize {
        crate::context_limits::model_context_limit(
            self.name(),
            if model.is_empty() || model == "default" {
                &self.default_model
            } else {
                model
            },
        )
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let endpoint = format!("{}/chat/completions", self.base_url);
        let model = if req.model.is_empty() || req.model == "default" {
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

                let content_val = if let Some(ref imgs) = m.images {
                    if imgs.is_empty() {
                        json!(m.content)
                    } else {
                        let mut parts = vec![json!({ "type": "text", "text": m.content })];
                        for img in imgs {
                            let url = if img.starts_with("data:") {
                                img.clone()
                            } else {
                                format!("data:image/png;base64,{}", img)
                            };
                            parts.push(json!({
                                "type": "image_url",
                                "image_url": { "url": url }
                            }));
                        }
                        json!(parts)
                    }
                } else {
                    json!(m.content)
                };

                let mut obj = json!({
                    "role": role_str,
                    "content": content_val,
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

        // Perform HTTP request asynchronously via piped curl stdin to prevent Windows quote mangling
        let base_url = endpoint.clone();
        let api_key = self.api_key.clone();

        let response_body =
            tokio::task::spawn_blocking(move || -> Result<String, ProviderError> {
                use std::io::Write;
                let curl_bin = if cfg!(target_os = "windows") {
                    "curl.exe"
                } else {
                    "curl"
                };
                let mut cmd = Command::new(curl_bin);
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

                cmd.arg("--data-binary")
                    .arg("@-")
                    .arg("-w")
                    .arg(format!("{}%{{http_code}}", CURL_STATUS_MARKER));
                cmd.stdin(std::process::Stdio::piped());
                cmd.stdout(std::process::Stdio::piped());
                cmd.stderr(std::process::Stdio::piped());

                let mut child = cmd
                    .spawn()
                    .map_err(|e| ProviderError::Network(e.to_string()))?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin
                        .write_all(body_str.as_bytes())
                        .map_err(|e| ProviderError::Network(e.to_string()))?;
                }

                let output = child
                    .wait_with_output()
                    .map_err(|e| ProviderError::Network(e.to_string()))?;

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


        let (response_body, http_status) = split_curl_response(response_body);
        let status_code = http_status.unwrap_or(200);

        let parsed: serde_json::Value = match serde_json::from_str(&response_body) {
            Ok(val) => val,
            Err(e) => {
                let sanitized_body = function_config::redact_secrets(&response_body);
                tracing::error!(
                    status = status_code,
                    body = %sanitized_body,
                    error = %e,
                    "Failed to parse LLM provider JSON response"
                );
                return Err(ProviderError::Api {
                    code: status_code,
                    message: format!(
                        "Invalid JSON response from provider (status {}): {}",
                        status_code,
                        sanitized_body.chars().take(500).collect::<String>()
                    ),
                });
            }
        };

        if status_code >= 400 || parsed.get("error").is_some() {
            let msg = provider_error_message(&parsed, &response_body);
            let sanitized_msg = function_config::redact_secrets(&msg);
            tracing::error!(
                status = status_code,
                error_message = %sanitized_msg,
                "LLM provider returned error response"
            );
            return Err(ProviderError::Api {
                code: if status_code >= 400 { status_code } else { 400 },
                message: sanitized_msg,
            });
        }

        let choices = parsed.get("choices").and_then(|c| c.as_array());
        let choice = match choices {
            Some(a) if !a.is_empty() => &a[0],
            _ => {
                let sanitized_body = function_config::redact_secrets(&response_body);
                tracing::error!(
                    status = status_code,
                    body = %sanitized_body,
                    "No completion choices returned in provider response"
                );
                return Err(ProviderError::Api {
                    code: status_code,
                    message: format!(
                        "No completion choices returned (status {}): {}",
                        status_code,
                        sanitized_body.chars().take(500).collect::<String>()
                    ),
                });
            }
        };

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        let msg_val = choice.get("message");

        let content = msg_val
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let tool_calls = if let Some(t_array) = msg_val.and_then(|m| m.get("tool_calls")).and_then(|t| t.as_array())
        {
            let mut calls = Vec::new();
            for c in t_array {
                let id = c
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(func) = c.get("function") {
                    let name = func
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args_val = func.get("arguments");
                    let arguments: serde_json::Value = match args_val {
                        Some(serde_json::Value::String(s)) => {
                            serde_json::from_str(s).unwrap_or(json!({ "raw": s }))
                        }
                        Some(val) => val.clone(),
                        None => json!({}),
                    };
                    calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
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
                images: None,
                tool_call_id: None,
                tool_calls,
            },
            finish_reason,
        })
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        mut on_token: Box<dyn FnMut(String) + Send>,
    ) -> Result<CompletionResponse, ProviderError> {
        if !req.tools.is_empty() {
            let res = self.complete(req).await?;
            if !res.message.content.is_empty() {
                on_token(res.message.content.clone());
            }
            return Ok(res);
        }

        let endpoint = format!("{}/chat/completions", self.base_url);
        let model = if req.model.is_empty() || req.model == "default" {
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
                let content_val = if let Some(ref imgs) = m.images {
                    if imgs.is_empty() {
                        json!(m.content)
                    } else {
                        let mut parts = vec![json!({ "type": "text", "text": m.content })];
                        for img in imgs {
                            let url = if img.starts_with("data:") {
                                img.clone()
                            } else {
                                format!("data:image/png;base64,{}", img)
                            };
                            parts.push(json!({
                                "type": "image_url",
                                "image_url": { "url": url }
                            }));
                        }
                        json!(parts)
                    }
                } else {
                    json!(m.content)
                };

                let mut obj = json!({
                    "role": role_str,
                    "content": content_val,
                });

                if let Some(ref t_id) = m.tool_call_id {
                    obj["tool_call_id"] = json!(t_id);
                }

                obj
            })
            .collect();

        let mut payload = json!({
            "model": model,
            "messages": messages_json,
            "stream": true,
        });

        if let Some(temp) = req.temperature {
            payload["temperature"] = json!(temp);
        }

        let body_str = payload.to_string();
        let base_url = endpoint.clone();
        let api_key = self.api_key.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let stream_task = tokio::task::spawn_blocking(move || -> Result<String, ProviderError> {
            use std::io::{BufRead, BufReader, Write};
            let curl_bin = if cfg!(target_os = "windows") {
                "curl.exe"
            } else {
                "curl"
            };
            let mut cmd = Command::new(curl_bin);
            cmd.arg("-s")
                .arg("-N")
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

            cmd.arg("--data-binary").arg("@-");
            cmd.stdin(std::process::Stdio::piped());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let mut child = cmd
                .spawn()
                .map_err(|e| ProviderError::Network(e.to_string()))?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(body_str.as_bytes())
                    .map_err(|e| ProviderError::Network(e.to_string()))?;
            }

            let mut accumulated = String::new();
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if let Ok(line_str) = line {
                        let trimmed = line_str.trim();
                        if trimmed.starts_with("data: ") {
                            let data = &trimmed[6..];
                            if data.trim() == "[DONE]" {
                                break;
                            }
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(delta) = val
                                    .pointer("/choices/0/delta/content")
                                    .and_then(|v| v.as_str())
                                {
                                    accumulated.push_str(delta);
                                    let _ = tx.send(delta.to_string());
                                }
                            }
                        }
                    }
                }
            }

            let _ = child.wait();
            Ok(accumulated)
        });

        while let Some(chunk) = rx.recv().await {
            on_token(chunk);
        }

        let full_content = stream_task
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))??;

        Ok(CompletionResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: full_content,
                images: None,
                tool_call_id: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
        })
    }
}

pub struct WhisperSttProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
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
            model: "whisper-1".to_string(),
        }
    }

    pub fn with_model(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        let mut provider = Self::new(base_url, api_key);
        provider.model = model.into();
        provider
    }
}

#[async_trait]
impl SpeechToTextProvider for WhisperSttProvider {
    fn name(&self) -> &str {
        "whisper"
    }

    async fn transcribe_audio(
        &self,
        audio_pcm: &[u8],
        _sample_rate: u32,
    ) -> Result<String, ProviderError> {
        if audio_pcm.is_empty() {
            return Ok(String::new());
        }

        let endpoint = format!("{}/audio/transcriptions", self.base_url);
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let bytes = audio_pcm.to_vec();

        tokio::task::spawn_blocking(move || -> Result<String, ProviderError> {
            let unique_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let temp_file = std::env::temp_dir().join(format!(
                "fn_audio_{}_{}.wav",
                std::process::id(),
                unique_id
            ));
            std::fs::write(&temp_file, &bytes)
                .map_err(|e| ProviderError::Network(format!("Failed to write audio: {}", e)))?;

            let curl_bin = if cfg!(target_os = "windows") {
                "curl.exe"
            } else {
                "curl"
            };
            let mut cmd = Command::new(curl_bin);
            cmd.arg("-s")
                .arg("-X")
                .arg("POST")
                .arg(&endpoint)
                .arg("-F")
                .arg(format!("file=@{}", temp_file.display()))
                .arg("-F")
                .arg(format!("model={model}"));

            if let Some(ref key) = api_key {
                if !key.is_empty() {
                    cmd.arg("-H").arg(format!("Authorization: Bearer {}", key));
                }
            }

            let output = cmd
                .output()
                .map_err(|e| ProviderError::Network(e.to_string()))?;
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
                let msg = err_obj
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Whisper API error");
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

pub struct OpenAiTtsProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    voice: String,
    output_format: String,
}

impl OpenAiTtsProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        voice: impl Into<String>,
        output_format: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            model: model.into(),
            voice: voice.into(),
            output_format: output_format.into(),
        }
    }
}

#[async_trait]
impl TextToSpeechProvider for OpenAiTtsProvider {
    fn name(&self) -> &str {
        "openai-compatible-tts"
    }

    async fn synthesize_speech(&self, text: &str) -> Result<Vec<u8>, ProviderError> {
        let base_url = format!("{}/audio/speech", self.base_url);
        let api_key = self.api_key.clone();
        let payload = json!({
            "model": self.model,
            "input": text,
            "voice": self.voice,
            "response_format": self.output_format,
        })
        .to_string();

        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let mut command = Command::new(if cfg!(target_os = "windows") { "curl.exe" } else { "curl" });
            command
                .args(["-s", "-X", "POST", &base_url, "-H", "Content-Type: application/json", "--data-binary", "@-"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped());
            if let Some(key) = api_key.filter(|key| !key.is_empty()) {
                command.arg("-H").arg(format!("Authorization: Bearer {key}"));
            }
            let mut child = command.spawn().map_err(|e| ProviderError::Network(e.to_string()))?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(payload.as_bytes()).map_err(|e| ProviderError::Network(e.to_string()))?;
            }
            let output = child.wait_with_output().map_err(|e| ProviderError::Network(e.to_string()))?;
            if !output.status.success() {
                return Err(ProviderError::Network(format!("curl exited with code {:?}", output.status.code())));
            }
            Ok(output.stdout)
        })
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_openai_call_structure() {
        let provider = OpenAiLlmProvider::new(
            "https://api.openai.com/v1",
            Some("test_key".to_string()),
            "gpt-4o",
        );
        assert_eq!(provider.name(), "openai-compatible");
    }

    #[test]
    fn test_provider_error_message_preserves_structured_error_details() {
        let body = r#"{"error":{"type":"invalid_request_error","code":"model_not_found"}}"#;
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();

        let message = provider_error_message(&parsed, body);

        assert!(message.contains("model_not_found"));
        assert_ne!(message, "Unknown provider error");
    }
}



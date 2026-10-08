//! OpenAI-compatible LLM and Speech-to-Text provider implementations.

use crate::{
    is_rate_limit_message, parse_retry_delay_from_message, ChatMessage, CompletionRequest,
    CompletionResponse, LlmProvider, MessageRole, ProviderError, SpeechToTextProvider,
    TextToSpeechProvider, ToolCall,
};
use async_trait::async_trait;
use serde_json::json;
use std::any::Any;
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

/// Returns true if the model name indicates vision capability.
pub fn is_vision_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.contains("vision")
        || lower.contains("gpt-4o")
        || lower.contains("gpt-4.1")
        || lower.contains("gemini")
        || lower.contains("claude-3")
        || lower.contains("llava")
}

/// Normalizes internal ChatMessage instances into provider-compatible JSON message objects.
pub fn normalize_chat_message(m: &ChatMessage, model: &str) -> serde_json::Value {
    let role_str = match m.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };

    let supports_vision = is_vision_model(model);

    let content_val = if m.role == MessageRole::User && supports_vision {
        if let Some(ref imgs) = m.images {
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
        }
    } else {
        // System, Assistant, Tool, or non-vision User messages MUST serialize content as a plain JSON string.
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
                let call_sig = c
                    .thought_signature
                    .as_ref()
                    .or(m.thought_signature.as_ref());

                let mut call_obj = json!({
                    "id": c.id,
                    "type": "function",
                    "function": {
                        "name": c.name,
                        "arguments": c.arguments.to_string(),
                    }
                });

                if let Some(ts) = call_sig {
                    call_obj["thought_signature"] = json!(ts);
                    call_obj["extra_fields"] = json!({
                        "thought_signature": ts
                    });
                    call_obj["function"]["thought_signature"] = json!(ts);
                    call_obj["function"]["extra_fields"] = json!({
                        "thought_signature": ts
                    });
                }

                call_obj
            })
            .collect();
        obj["tool_calls"] = json!(calls_json);
    }

    if let Some(ref ts) = m.thought_signature.as_ref().or_else(|| {
        m.tool_calls
            .as_ref()
            .and_then(|calls| calls.iter().find_map(|c| c.thought_signature.as_ref()))
    }) {
        obj["thought_signature"] = json!(ts);
        obj["extra_fields"] = json!({
            "thought_signature": ts
        });
    }

    obj
}

/// Validates normalized message JSON objects immediately prior to sending HTTP requests.
pub fn validate_chat_messages(messages_json: &[serde_json::Value]) -> Result<(), ProviderError> {
    for (idx, msg) in messages_json.iter().enumerate() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
        let content = msg.get("content");

        match role {
            "system" | "assistant" | "tool" => {
                if let Some(c) = content {
                    if !c.is_string() {
                        let content_type = match c {
                            serde_json::Value::Null => "null",
                            serde_json::Value::Bool(_) => "bool",
                            serde_json::Value::Number(_) => "number",
                            serde_json::Value::Array(_) => "array",
                            serde_json::Value::Object(_) => "object",
                            serde_json::Value::String(_) => "string",
                        };
                        tracing::error!(
                            index = idx,
                            role = %role,
                            content_type = %content_type,
                            "Invalid non-string message content before provider serialization"
                        );
                        return Err(ProviderError::Api {
                            code: 400,
                            message: format!(
                                "Message at index {} (role '{}') has invalid non-string content type '{}'",
                                idx, role, content_type
                            ),
                        });
                    }
                } else {
                    tracing::error!(
                        index = idx,
                        role = %role,
                        "Missing content field in message before provider serialization"
                    );
                    return Err(ProviderError::Api {
                        code: 400,
                        message: format!("Message at index {} (role '{}') is missing content field", idx, role),
                    });
                }
            }
            "user" => {
                if let Some(c) = content {
                    if !c.is_string() && !c.is_array() {
                        let content_type = match c {
                            serde_json::Value::Null => "null",
                            serde_json::Value::Bool(_) => "bool",
                            serde_json::Value::Number(_) => "number",
                            serde_json::Value::Array(_) => "array",
                            serde_json::Value::Object(_) => "object",
                            serde_json::Value::String(_) => "string",
                        };
                        tracing::error!(
                            index = idx,
                            role = %role,
                            content_type = %content_type,
                            "Invalid user message content type before provider serialization"
                        );
                        return Err(ProviderError::Api {
                            code: 400,
                            message: format!(
                                "Message at index {} (role 'user') has invalid content type '{}'",
                                idx, content_type
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
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

    fn as_any(&self) -> &dyn Any {
        self
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
            .map(|m| normalize_chat_message(m, model))
            .collect();

        validate_chat_messages(&messages_json)?;

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
                    http_status = status_code,
                    raw_response = %sanitized_body,
                    "Failed to parse provider response JSON: {}",
                    e
                );
                if status_code == 429 || is_rate_limit_message(&response_body) {
                    let retry_after = parse_retry_delay_from_message(&response_body);
                    return Err(ProviderError::RateLimit {
                        code: status_code,
                        message: response_body,
                        retry_after,
                    });
                }
                return Err(ProviderError::Api {
                    code: status_code,
                    message: format!("Invalid JSON response from provider: {}", e),
                });
            }
        };

        if parsed.get("error").is_some() || status_code >= 400 {
            let msg = provider_error_message(&parsed, &response_body);
            let sanitized_msg = function_config::redact_secrets(&msg);
            let sanitized_body = function_config::redact_secrets(&response_body);
            tracing::error!(
                http_status = status_code,
                error_message = %sanitized_msg,
                raw_response = %sanitized_body,
                "Provider returned HTTP error status or error payload"
            );
            if status_code == 429 || is_rate_limit_message(&msg) || is_rate_limit_message(&response_body) {
                let retry_after = parse_retry_delay_from_message(&msg)
                    .or_else(|| parse_retry_delay_from_message(&response_body));
                return Err(ProviderError::RateLimit {
                    code: status_code,
                    message: msg,
                    retry_after,
                });
            }
            return Err(ProviderError::Api {
                code: status_code,
                message: msg,
            });
        }

        let choices_array = parsed
            .get("choices")
            .or_else(|| parsed.get("candidates"))
            .and_then(|c| c.as_array());

        let choice = match choices_array.and_then(|a| a.first()) {
            Some(c) => c,
            None => {
                let sanitized_body = function_config::redact_secrets(&response_body);
                tracing::error!(
                    http_status = status_code,
                    raw_response = %sanitized_body,
                    "Provider response contained no completion choices or candidates"
                );
                return Err(ProviderError::Api {
                    code: status_code,
                    message: if response_body.trim().is_empty() {
                        "Empty response received from provider".to_string()
                    } else {
                        "No completion choices returned by provider".to_string()
                    },
                });
            }
        };

        let msg_val = choice
            .get("message")
            .or_else(|| choice.get("content"))
            .unwrap_or(choice);

        let finish_reason = choice
            .get("finish_reason")
            .or_else(|| choice.get("finishReason"))
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        let thought_signature = msg_val
            .get("thought_signature")
            .or_else(|| choice.get("thought_signature"))
            .or_else(|| msg_val.pointer("/extra_fields/thought_signature"))
            .or_else(|| choice.pointer("/extra_fields/thought_signature"))
            .or_else(|| parsed.get("thought_signature"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut content = String::new();
        if let Some(c_str) = msg_val.get("content").and_then(|c| c.as_str()) {
            content.push_str(c_str);
        } else if let Some(c_arr) = msg_val.get("content").and_then(|c| c.as_array()) {
            for part in c_arr {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    content.push_str(text);
                }
            }
        }

        if content.is_empty() {
            if let Some(parts) = msg_val
                .get("parts")
                .or_else(|| choice.get("parts"))
                .and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        content.push_str(text);
                    }
                }
            }
        }

        let mut calls = Vec::new();

        if let Some(t_array) = msg_val.get("tool_calls").and_then(|t| t.as_array()) {
            for (idx, c) in t_array.iter().enumerate() {
                let id = c
                    .get("id")
                    .and_then(|i| i.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("call_{}", idx));

                if let Some(func) = c.get("function") {
                    let name = func
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = match func.get("arguments") {
                        Some(serde_json::Value::String(args_str)) => serde_json::from_str(args_str)
                            .unwrap_or_else(|_| json!({ "raw": args_str })),
                        Some(val) => val.clone(),
                        None => json!({}),
                    };
                    let call_thought_signature = c
                        .get("thought_signature")
                        .or_else(|| func.get("thought_signature"))
                        .or_else(|| c.pointer("/extra_fields/thought_signature"))
                        .or_else(|| func.pointer("/extra_fields/thought_signature"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| thought_signature.clone());

                    if !name.is_empty() {
                        calls.push(ToolCall {
                            id,
                            name,
                            arguments,
                            thought_signature: call_thought_signature,
                        });
                    }
                }
            }
        }

        if calls.is_empty() {
            if let Some(func) = msg_val.get("function_call") {
                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                    let arguments = match func.get("arguments") {
                        Some(serde_json::Value::String(args_str)) => serde_json::from_str(args_str)
                            .unwrap_or_else(|_| json!({ "raw": args_str })),
                        Some(val) => val.clone(),
                        None => json!({}),
                    };
                    let call_thought_signature = func
                        .get("thought_signature")
                        .or_else(|| msg_val.get("thought_signature"))
                        .or_else(|| func.pointer("/extra_fields/thought_signature"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| thought_signature.clone());

                    calls.push(ToolCall {
                        id: "call_0".to_string(),
                        name: name.to_string(),
                        arguments,
                        thought_signature: call_thought_signature,
                    });
                }
            }
        }

        if calls.is_empty() {
            let parts_opt = msg_val
                .get("parts")
                .or_else(|| choice.get("parts"))
                .or_else(|| {
                    msg_val
                        .get("content")
                        .and_then(|c| c.as_array().map(|_| msg_val.get("content").unwrap()))
                });

            if let Some(parts) = parts_opt.and_then(|p| p.as_array()) {
                for (idx, part) in parts.iter().enumerate() {
                    let fc = part
                        .get("functionCall")
                        .or_else(|| part.get("function_call"));
                    if let Some(fc) = fc {
                        if let Some(name) = fc.get("name").and_then(|n| n.as_str()) {
                            let arguments = fc
                                .get("args")
                                .or_else(|| fc.get("arguments"))
                                .cloned()
                                .unwrap_or(json!({}));
                            let call_thought_signature = part
                                .get("thought_signature")
                                .or_else(|| fc.get("thought_signature"))
                                .or_else(|| part.pointer("/extra_fields/thought_signature"))
                                .or_else(|| fc.pointer("/extra_fields/thought_signature"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| thought_signature.clone());

                            let id = format!("call_{}", idx);
                            calls.push(ToolCall {
                                id,
                                name: name.to_string(),
                                arguments,
                                thought_signature: call_thought_signature,
                            });
                        }
                    }
                }
            }
        }

        let final_thought_signature =
            thought_signature.or_else(|| calls.iter().find_map(|c| c.thought_signature.clone()));

        let tool_calls = if calls.is_empty() { None } else { Some(calls) };

        Ok(CompletionResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content,
                images: None,
                tool_call_id: None,
                tool_calls,
                thought_signature: final_thought_signature,
            },
            finish_reason,
        })
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        mut on_token: Box<dyn FnMut(String) + Send>,
    ) -> Result<CompletionResponse, ProviderError> {
        let endpoint = format!("{}/chat/completions", self.base_url);
        let model = if req.model.is_empty() || req.model == "default" {
            &self.default_model
        } else {
            &req.model
        };

        let messages_json: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| normalize_chat_message(m, model))
            .collect();

        validate_chat_messages(&messages_json)?;

        let mut payload = json!({
            "model": model,
            "messages": messages_json,
            "stream": true,
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
        let base_url = endpoint.clone();
        let api_key = self.api_key.clone();

        enum StreamEvent {
            Token(String),
            Error(u16, String),
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();

        let stream_task = tokio::task::spawn_blocking(move || -> Result<CompletionResponse, ProviderError> {
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

            let mut accumulated_content = String::new();
            let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
            let mut thought_signature: Option<String> = None;
            let mut finish_reason: Option<String> = None;
            let mut current_event_type: Option<String> = None;
            let mut http_status: Option<u16> = None;
            let mut raw_response_buffer = String::new();

            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line_res in reader.lines() {
                    let line_str = match line_res {
                        Ok(l) => l,
                        Err(e) => {
                            tracing::warn!("Failed to read SSE line: {}", e);
                            continue;
                        }
                    };

                    raw_response_buffer.push_str(&line_str);
                    raw_response_buffer.push('\n');

                    let mut line_to_process = line_str.as_str();
                    if let Some((body_part, status_part)) = line_to_process.rsplit_once(CURL_STATUS_MARKER) {
                        if let Ok(code) = status_part.trim().parse::<u16>() {
                            http_status = Some(code);
                        }
                        line_to_process = body_part;
                    }

                    let trimmed = line_to_process.trim();
                    if trimmed.is_empty() {
                        current_event_type = None;
                        continue;
                    }

                    if trimmed.starts_with("event:") {
                        let ev_type = trimmed["event:".len()..].trim().to_string();
                        tracing::debug!(raw_event_type = %ev_type, "Received SSE event type");
                        current_event_type = Some(ev_type);
                        continue;
                    }

                    if trimmed.starts_with("data:") {
                        let data_str = trimmed["data:".len()..].trim();
                        tracing::debug!(
                            event_type = ?current_event_type,
                            data_len = data_str.len(),
                            "Processing SSE data chunk"
                        );

                        if data_str == "[DONE]" {
                            break;
                        }

                        if current_event_type.as_deref() == Some("error") {
                            let status_code = http_status.unwrap_or(200);
                            let err_msg = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data_str) {
                                provider_error_message(&parsed, data_str)
                            } else {
                                data_str.to_string()
                            };
                            let sanitized_msg = function_config::redact_secrets(&err_msg);
                            tracing::warn!(
                                http_status = status_code,
                                error_message = %sanitized_msg,
                                raw_data = %function_config::redact_secrets(data_str),
                                "Provider emitted error event in SSE stream"
                            );
                            let _ = tx.send(StreamEvent::Error(status_code, err_msg.clone()));
                            return Err(ProviderError::Api {
                                code: status_code,
                                message: err_msg,
                            });
                        }

                        match serde_json::from_str::<serde_json::Value>(data_str) {
                            Ok(val) => {
                                if val.get("error").is_some() {
                                    let status_code = http_status.unwrap_or(200);
                                    let err_msg = provider_error_message(&val, data_str);
                                    let sanitized_msg = function_config::redact_secrets(&err_msg);
                                    tracing::warn!(
                                        http_status = status_code,
                                        error_message = %sanitized_msg,
                                        raw_data = %function_config::redact_secrets(data_str),
                                        "JSON error injected into SSE stream data"
                                    );
                                    let _ = tx.send(StreamEvent::Error(status_code, err_msg.clone()));
                                    return Err(ProviderError::Api {
                                        code: status_code,
                                        message: err_msg,
                                    });
                                }

                                if let Some(sig) = val
                                    .pointer("/choices/0/delta/thought_signature")
                                    .or_else(|| val.pointer("/choices/0/thought_signature"))
                                    .and_then(|v| v.as_str())
                                {
                                    thought_signature = Some(sig.to_string());
                                }

                                if let Some(fr) = val
                                    .pointer("/choices/0/finish_reason")
                                    .and_then(|v| v.as_str())
                                {
                                    finish_reason = Some(fr.to_string());
                                }

                                if let Some(delta) = val
                                    .pointer("/choices/0/delta/content")
                                    .and_then(|v| v.as_str())
                                {
                                    accumulated_content.push_str(delta);
                                    let _ = tx.send(StreamEvent::Token(delta.to_string()));
                                }

                                if let Some(tool_calls_arr) = val
                                    .pointer("/choices/0/delta/tool_calls")
                                    .and_then(|v| v.as_array())
                                {
                                    for tc_val in tool_calls_arr {
                                        let idx = tc_val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                        while accumulated_tool_calls.len() <= idx {
                                            accumulated_tool_calls.push(ToolCall {
                                                id: format!("call_{}", accumulated_tool_calls.len()),
                                                name: String::new(),
                                                arguments: json!({}),
                                                thought_signature: None,
                                            });
                                        }

                                        if let Some(id_str) = tc_val.get("id").and_then(|i| i.as_str()) {
                                            accumulated_tool_calls[idx].id = id_str.to_string();
                                        }

                                        if let Some(func) = tc_val.get("function") {
                                            if let Some(name_str) = func.get("name").and_then(|n| n.as_str()) {
                                                accumulated_tool_calls[idx].name.push_str(name_str);
                                            }
                                            if let Some(args_str) = func.get("arguments").and_then(|a| a.as_str()) {
                                                let curr_args = match &accumulated_tool_calls[idx].arguments {
                                                    serde_json::Value::String(s) => format!("{}{}", s, args_str),
                                                    serde_json::Value::Object(map) if map.is_empty() => args_str.to_string(),
                                                    val => format!("{}{}", val.as_str().unwrap_or(""), args_str),
                                                };
                                                accumulated_tool_calls[idx].arguments = serde_json::Value::String(curr_args);
                                            }
                                        }
                                    }
                                }

                            }
                            Err(parse_err) => {
                                let status_code = http_status.unwrap_or(200);
                                tracing::warn!(
                                    http_status = status_code,
                                    parse_error = %parse_err,
                                    raw_data = %function_config::redact_secrets(data_str),
                                    "Failed to parse SSE JSON payload"
                                );
                                if current_event_type.as_deref() == Some("error") || status_code >= 400 {
                                    let err_msg = format!("Non-JSON SSE payload: {}", data_str);
                                    let _ = tx.send(StreamEvent::Error(status_code, err_msg.clone()));
                                    return Err(ProviderError::Api {
                                        code: status_code,
                                        message: err_msg,
                                    });
                                }
                            }
                        }
                        continue;
                    }

                    if trimmed.starts_with('{') {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if val.get("error").is_some() {
                                let status_code = http_status.unwrap_or(400);
                                let err_msg = provider_error_message(&val, trimmed);
                                let sanitized_msg = function_config::redact_secrets(&err_msg);
                                tracing::warn!(
                                    http_status = status_code,
                                    error_message = %sanitized_msg,
                                    "Direct JSON error response received during SSE stream"
                                );
                                let _ = tx.send(StreamEvent::Error(status_code, err_msg.clone()));
                                return Err(ProviderError::Api {
                                    code: status_code,
                                    message: err_msg,
                                });
                            }
                        }
                    }
                }
            }

            let _ = child.wait();

            let status_code = http_status.unwrap_or(200);
            if status_code >= 400 {
                let err_msg = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_response_buffer) {
                    provider_error_message(&val, &raw_response_buffer)
                } else if raw_response_buffer.trim().is_empty() {
                    format!("HTTP status {}", status_code)
                } else {
                    raw_response_buffer.trim().chars().take(1000).collect()
                };
                let _ = tx.send(StreamEvent::Error(status_code, err_msg.clone()));
                return Err(ProviderError::Api {
                    code: status_code,
                    message: err_msg,
                });
            }

            for tc in &mut accumulated_tool_calls {
                if let serde_json::Value::String(s) = &tc.arguments {
                    tc.arguments = serde_json::from_str(s).unwrap_or_else(|_| json!({ "raw": s }));
                }
            }

            let tool_calls = if accumulated_tool_calls.is_empty() {
                None
            } else {
                Some(accumulated_tool_calls)
            };

            Ok(CompletionResponse {
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content: accumulated_content,
                    images: None,
                    tool_call_id: None,
                    tool_calls,
                    thought_signature,
                },
                finish_reason: finish_reason.or_else(|| Some("stop".to_string())),
            })
        });

        let mut stream_error = None;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(token) => on_token(token),
                StreamEvent::Error(code, message) => {
                    stream_error = Some(ProviderError::Api { code, message });
                    break;
                }
            }
        }

        if let Some(err) = stream_error {
            return Err(err);
        }

        stream_task
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?
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
                let status_code = err_obj.get("code").and_then(|c| c.as_u64()).unwrap_or(400) as u16;
                if status_code == 429 || is_rate_limit_message(msg) {
                    return Err(ProviderError::RateLimit {
                        code: status_code,
                        message: msg.to_string(),
                        retry_after: parse_retry_delay_from_message(msg),
                    });
                }
                return Err(ProviderError::Api {
                    code: status_code,
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
            let mut command = Command::new(if cfg!(target_os = "windows") {
                "curl.exe"
            } else {
                "curl"
            });
            command
                .args([
                    "-s",
                    "-X",
                    "POST",
                    &base_url,
                    "-H",
                    "Content-Type: application/json",
                    "--data-binary",
                    "@-",
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped());
            if let Some(key) = api_key.filter(|key| !key.is_empty()) {
                command
                    .arg("-H")
                    .arg(format!("Authorization: Bearer {key}"));
            }
            let mut child = command
                .spawn()
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(payload.as_bytes())
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
    fn test_normalize_normal_user_assistant_messages() {
        let sys = ChatMessage::system("System instructions");
        let user = ChatMessage::user("User question");
        let asst = ChatMessage::assistant("Assistant reply");

        let model = "llama-3.3-70b-versatile";
        let sys_json = normalize_chat_message(&sys, model);
        let user_json = normalize_chat_message(&user, model);
        let asst_json = normalize_chat_message(&asst, model);

        assert_eq!(sys_json["role"], "system");
        assert_eq!(sys_json["content"], "System instructions");

        assert_eq!(user_json["role"], "user");
        assert_eq!(user_json["content"], "User question");

        assert_eq!(asst_json["role"], "assistant");
        assert_eq!(asst_json["content"], "Assistant reply");

        assert!(validate_chat_messages(&[sys_json, user_json, asst_json]).is_ok());
    }

    #[test]
    fn test_normalize_compacted_conversation_summaries() {
        let summary_msg = ChatMessage::user("Summary of previous turns:\n- User asked to list files\n- System returned 3 files");
        let model = "llama-3.3-70b-versatile";
        let json_val = normalize_chat_message(&summary_msg, model);

        assert_eq!(json_val["role"], "user");
        assert!(json_val["content"].is_string());
        assert!(json_val["content"].as_str().unwrap().contains("Summary of previous turns"));
        assert!(validate_chat_messages(&[json_val]).is_ok());
    }

    #[test]
    fn test_normalize_tool_calls_and_tool_results() {
        let tool_call_msg = ChatMessage {
            role: MessageRole::Assistant,
            content: "".to_string(),
            images: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_abc123".to_string(),
                name: "take_screenshot".to_string(),
                arguments: json!({}),
                thought_signature: None,
            }]),
            thought_signature: None,
        };

        let tool_result_msg = ChatMessage::tool("call_abc123", "{\"status\": \"ok\", \"width\": 1920}");

        let model = "llama-3.3-70b-versatile";
        let call_json = normalize_chat_message(&tool_call_msg, model);
        let result_json = normalize_chat_message(&tool_result_msg, model);

        assert_eq!(call_json["role"], "assistant");
        assert_eq!(call_json["content"], "");
        assert_eq!(call_json["tool_calls"][0]["id"], "call_abc123");
        assert_eq!(call_json["tool_calls"][0]["function"]["name"], "take_screenshot");

        assert_eq!(result_json["role"], "tool");
        assert_eq!(result_json["tool_call_id"], "call_abc123");
        assert!(result_json["content"].is_string());

        assert!(validate_chat_messages(&[call_json, result_json]).is_ok());
    }

    #[test]
    fn test_normalize_multiple_tool_call_result_cycles() {
        let msgs = vec![
            ChatMessage::system("System prompt"),
            ChatMessage::user("Do task"),
            ChatMessage {
                role: MessageRole::Assistant,
                content: "".to_string(),
                images: None,
                tool_call_id: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "open_app".to_string(),
                    arguments: json!({ "name": "Chrome" }),
                    thought_signature: None,
                }]),
                thought_signature: None,
            },
            ChatMessage::tool("call_1", "App opened"),
            ChatMessage {
                role: MessageRole::Assistant,
                content: "".to_string(),
                images: None,
                tool_call_id: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_2".to_string(),
                    name: "take_screenshot".to_string(),
                    arguments: json!({}),
                    thought_signature: None,
                }]),
                thought_signature: None,
            },
            ChatMessage::tool("call_2", "Screenshot captured"),
            ChatMessage::user_with_images("Visual observation:", vec!["base64data".to_string()]),
            ChatMessage::assistant("Finished task!"),
        ];

        let model = "llama-3.3-70b-versatile";
        let normalized: Vec<_> = msgs.iter().map(|m| normalize_chat_message(m, model)).collect();

        // Check index 6 (the user_with_images message) on non-vision model normalizes to plain string content
        assert_eq!(normalized[6]["role"], "user");
        assert!(normalized[6]["content"].is_string());
        assert_eq!(normalized[6]["content"], "Visual observation:");

        assert!(validate_chat_messages(&normalized).is_ok());
    }

    #[test]
    fn test_validate_chat_messages_catches_invalid_content_type() {
        let invalid_msg = json!({
            "role": "tool",
            "content": ["invalid", "array", "in", "tool", "content"]
        });

        let err = validate_chat_messages(&[invalid_msg]).unwrap_err();
        match err {
            ProviderError::Api { message, .. } => {
                assert!(message.contains("index 0"));
                assert!(message.contains("role 'tool'"));
                assert!(message.contains("array"));
            }
            _ => panic!("Expected ProviderError::Api"),
        }
    }

    #[test]
    fn test_user_with_images_on_vision_model_serializes_array() {
        let msg = ChatMessage::user_with_images("Visual observation:", vec!["b64data".to_string()]);
        let model = "gpt-4o";
        let json_val = normalize_chat_message(&msg, model);

        assert_eq!(json_val["role"], "user");
        assert!(json_val["content"].is_array());
        assert_eq!(json_val["content"][0]["type"], "text");
        assert_eq!(json_val["content"][1]["type"], "image_url");

        assert!(validate_chat_messages(&[json_val]).is_ok());
    }

    #[test]
    fn test_provider_error_message_preserves_structured_error_details() {
        let body = r#"{"error":{"type":"invalid_request_error","code":"model_not_found"}}"#;
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();

        let message = provider_error_message(&parsed, body);

        assert!(message.contains("model_not_found"));
        assert_ne!(message, "Unknown provider error");
    }

    #[test]
    fn test_gemini_normal_text_response() {
        let json_resp = json!({
            "choices": [
                {
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "Hello! How can I help you today?"
                    }
                }
            ]
        });

        let raw = format!("{}{}", json_resp, CURL_STATUS_MARKER);
        let (body, status) = split_curl_response(raw);
        assert_eq!(status, None);

        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let choice = &parsed["choices"][0];
        let content = choice["message"]["content"].as_str().unwrap();
        assert_eq!(content, "Hello! How can I help you today?");
    }

    #[test]
    fn test_gemini_function_call_and_thought_signature() {
        let json_resp = json!({
            "choices": [
                {
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "thought_signature": "sig_abc123_gemini_thought",
                        "tool_calls": [
                            {
                                "id": "call_12345",
                                "type": "function",
                                "function": {
                                    "name": "take_screenshot",
                                    "arguments": "{}"
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let parsed: serde_json::Value = json_resp;
        let choice = &parsed["choices"][0];
        let msg_val = &choice["message"];
        let thought_sig = msg_val.get("thought_signature").and_then(|v| v.as_str());
        assert_eq!(thought_sig, Some("sig_abc123_gemini_thought"));

        let content = msg_val
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        assert_eq!(content, "");

        let tool_calls = msg_val
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "take_screenshot");
    }

    #[test]
    fn test_gemini_request_serialization_includes_thought_signature() {
        let msg = ChatMessage {
            role: MessageRole::Assistant,
            content: "".to_string(),
            images: None,
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_abc".to_string(),
                name: "click".to_string(),
                arguments: json!({ "x": 100, "y": 200 }),
                thought_signature: None,
            }]),
            thought_signature: Some("gemini_thought_sig_xyz".to_string()),
        };

        let mut obj = json!({
            "role": "assistant",
            "content": msg.content,
        });
        if let Some(ref ts) = msg.thought_signature {
            obj["thought_signature"] = json!(ts);
            obj["extra_fields"] = json!({
                "thought_signature": ts
            });
        }

        assert_eq!(obj["thought_signature"], "gemini_thought_sig_xyz");
        assert_eq!(
            obj["extra_fields"]["thought_signature"],
            "gemini_thought_sig_xyz"
        );
    }

    #[test]
    fn test_gemini_thought_signature_end_to_end_serialization_deserialization() {
        let raw_gemini_resp = json!({
            "choices": [
                {
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "thought_signature": "gemini_sig_end_to_end_123",
                        "tool_calls": [
                            {
                                "id": "call_open_app_1",
                                "type": "function",
                                "function": {
                                    "name": "open_app",
                                    "arguments": "{\"name\":\"Google Chrome\"}"
                                },
                                "thought_signature": "gemini_sig_end_to_end_123"
                            }
                        ]
                    }
                }
            ]
        });

        let choice = &raw_gemini_resp["choices"][0];
        let msg_val = &choice["message"];

        let thought_signature = msg_val
            .get("thought_signature")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        assert_eq!(
            thought_signature,
            Some("gemini_sig_end_to_end_123".to_string())
        );

        let t_array = msg_val["tool_calls"].as_array().unwrap();
        let mut calls = Vec::new();
        for (_idx, c) in t_array.iter().enumerate() {
            let id = c.get("id").and_then(|i| i.as_str()).unwrap().to_string();
            let func = &c["function"];
            let name = func["name"].as_str().unwrap().to_string();
            let args_str = func["arguments"].as_str().unwrap();
            let arguments: serde_json::Value = serde_json::from_str(args_str).unwrap();
            let call_thought_signature = c
                .get("thought_signature")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| thought_signature.clone());

            calls.push(ToolCall {
                id,
                name,
                arguments,
                thought_signature: call_thought_signature,
            });
        }

        let assistant_msg = ChatMessage {
            role: MessageRole::Assistant,
            content: "".to_string(),
            images: None,
            tool_call_id: None,
            tool_calls: Some(calls),
            thought_signature,
        };

        // Now simulate request payload serialization for the subsequent Gemini request
        let t_calls = assistant_msg.tool_calls.as_ref().unwrap();
        let calls_json: Vec<serde_json::Value> = t_calls
            .iter()
            .map(|c| {
                let call_sig = c
                    .thought_signature
                    .as_ref()
                    .or(assistant_msg.thought_signature.as_ref());

                let mut call_obj = json!({
                    "id": c.id,
                    "type": "function",
                    "function": {
                        "name": c.name,
                        "arguments": c.arguments.to_string(),
                    }
                });

                if let Some(ts) = call_sig {
                    call_obj["thought_signature"] = json!(ts);
                    call_obj["extra_fields"] = json!({
                        "thought_signature": ts
                    });
                    call_obj["function"]["thought_signature"] = json!(ts);
                    call_obj["function"]["extra_fields"] = json!({
                        "thought_signature": ts
                    });
                }

                call_obj
            })
            .collect();

        let mut req_msg_obj = json!({
            "role": "assistant",
            "content": assistant_msg.content,
            "tool_calls": calls_json,
        });

        if let Some(ref ts) = assistant_msg.thought_signature {
            req_msg_obj["thought_signature"] = json!(ts);
            req_msg_obj["extra_fields"] = json!({
                "thought_signature": ts
            });
        }

        assert_eq!(
            req_msg_obj["thought_signature"],
            "gemini_sig_end_to_end_123"
        );
        assert_eq!(
            req_msg_obj["extra_fields"]["thought_signature"],
            "gemini_sig_end_to_end_123"
        );
        assert_eq!(
            req_msg_obj["tool_calls"][0]["thought_signature"],
            "gemini_sig_end_to_end_123"
        );
        assert_eq!(
            req_msg_obj["tool_calls"][0]["extra_fields"]["thought_signature"],
            "gemini_sig_end_to_end_123"
        );
        assert_eq!(
            req_msg_obj["tool_calls"][0]["function"]["thought_signature"],
            "gemini_sig_end_to_end_123"
        );
        assert_eq!(
            req_msg_obj["tool_calls"][0]["function"]["extra_fields"]["thought_signature"],
            "gemini_sig_end_to_end_123"
        );
    }

    #[test]
    fn test_gemini_multiple_sequential_tool_calls_parsing() {
        let json_resp = json!({
            "candidates": [
                {
                    "finishReason": "STOP",
                    "content": {
                        "parts": [
                            {
                                "functionCall": {
                                    "name": "open_app",
                                    "args": { "name": "Chrome" }
                                }
                            },
                            {
                                "functionCall": {
                                    "name": "take_screenshot",
                                    "args": {}
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let choice = &json_resp["candidates"][0];
        let msg_val = &choice["content"];
        let parts = msg_val["parts"].as_array().unwrap();

        let mut calls = Vec::new();
        for (idx, part) in parts.iter().enumerate() {
            if let Some(fc) = part.get("functionCall") {
                if let Some(name) = fc.get("name").and_then(|n| n.as_str()) {
                    let arguments = fc.get("args").cloned().unwrap_or(json!({}));
                    calls.push(ToolCall {
                        id: format!("call_{}", idx),
                        name: name.to_string(),
                        arguments,
                        thought_signature: None,
                    });
                }
            }
        }

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "open_app");
        assert_eq!(calls[1].name, "take_screenshot");
    }

    #[tokio::test]
    async fn test_sse_stream_handles_error_event() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                use std::io::Write;
                let sse_response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\nevent: error\r\ndata: {\"error\": {\"message\": \"JSON error injected into SSE stream\"}}\n\n";
                let _ = socket.write_all(sse_response.as_bytes());
            }
        });

        let provider = OpenAiLlmProvider::new(
            format!("http://{}", addr),
            Some("test_key".to_string()),
            "gpt-4o",
        );

        let req = CompletionRequest {
            model: "default".to_string(),
            messages: vec![ChatMessage::user("hi")],
            tools: vec![],
            temperature: None,
        };

        let result = provider.complete_stream(req, Box::new(|_| {})).await;
        assert!(result.is_err());
        if let Err(ProviderError::Api { code, message }) = result {
            assert_eq!(code, 200);
            assert!(message.contains("JSON error injected into SSE stream"));
        } else {
            panic!("Expected ProviderError::Api, got {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_sse_stream_handles_injected_json_error_in_data() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                use std::io::Write;
                let sse_response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {\"error\": {\"message\": \"Direct JSON error in data payload\", \"code\": 500}}\n\n";
                let _ = socket.write_all(sse_response.as_bytes());
            }
        });

        let provider = OpenAiLlmProvider::new(
            format!("http://{}", addr),
            Some("test_key".to_string()),
            "gpt-4o",
        );

        let req = CompletionRequest {
            model: "default".to_string(),
            messages: vec![ChatMessage::user("hi")],
            tools: vec![],
            temperature: None,
        };

        let result = provider.complete_stream(req, Box::new(|_| {})).await;
        assert!(result.is_err());
        if let Err(ProviderError::Api { message, .. }) = result {
            assert!(message.contains("Direct JSON error in data payload"));
        } else {
            panic!("Expected ProviderError::Api, got {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_sse_stream_successful_tokens() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                use std::io::Write;
                let sse_response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {\"choices\": [{\"delta\": {\"content\": \"Hello \"}}]}\n\ndata: {\"choices\": [{\"delta\": {\"content\": \"world\"}}]}\n\ndata: [DONE]\n\n";
                let _ = socket.write_all(sse_response.as_bytes());
            }
        });

        let provider = OpenAiLlmProvider::new(
            format!("http://{}", addr),
            Some("test_key".to_string()),
            "gpt-4o",
        );

        let req = CompletionRequest {
            model: "default".to_string(),
            messages: vec![ChatMessage::user("hi")],
            tools: vec![],
            temperature: None,
        };

        let res = provider.complete_stream(req, Box::new(|_| {})).await.unwrap();
        assert_eq!(res.message.content, "Hello world");
    }

    #[test]
    fn test_malformed_or_empty_provider_response_gives_descriptive_error() {
        let body = "";
        let (resp, status) = split_curl_response(format!("{}{}", body, CURL_STATUS_MARKER));
        assert_eq!(resp, "");
        assert_eq!(status, None);

        let body_404 = r#"{"error": {"code": 404, "message": "Model not found"}}"#;
        let parsed_404: serde_json::Value = serde_json::from_str(body_404).unwrap();
        let msg_404 = provider_error_message(&parsed_404, body_404);
        assert_eq!(msg_404, "Model not found");
    }
}

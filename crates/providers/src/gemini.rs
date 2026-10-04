use crate::{
    context_limits::model_context_limit,
    openai::{extract_raw_function_call, find_thought_signature},
    ChatMessage, CompletionRequest, CompletionResponse, LlmProvider, MessageRole, OpenAiLlmProvider,
    ProviderError, ToolCall,
};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::process::Command;

pub const GEMINI_OPENAI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/";
pub const GEMINI_NATIVE_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

const CURL_STATUS_MARKER: &str = "\n__FUNCTION_HTTP_STATUS__:";

fn split_curl_response(raw_response: String) -> (String, Option<u16>) {
    if let Some((body, status)) = raw_response.rsplit_once(CURL_STATUS_MARKER) {
        return (body.to_string(), status.trim().parse().ok());
    }
    (raw_response, None)
}

pub fn build_gemini_native_payload(req: &CompletionRequest) -> serde_json::Value {
    let mut system_parts = Vec::new();
    let mut contents = Vec::new();
    let mut call_id_to_name = HashMap::new();

    for m in &req.messages {
        match m.role {
            MessageRole::System => {
                if !m.content.trim().is_empty() {
                    system_parts.push(json!({ "text": m.content }));
                }
            }
            MessageRole::User => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                if let Some(ref imgs) = m.images {
                    for img in imgs {
                        let data_str = if let Some(stripped) = img.strip_prefix("data:") {
                            stripped.split_once(',').map(|(_, b64)| b64).unwrap_or(img)
                        } else {
                            img.as_str()
                        };
                        parts.push(json!({
                            "inlineData": {
                                "mimeType": "image/png",
                                "data": data_str
                            }
                        }));
                    }
                }
                if parts.is_empty() {
                    parts.push(json!({ "text": "" }));
                }
                contents.push(json!({
                    "role": "user",
                    "parts": parts
                }));
            }
            MessageRole::Assistant => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                if let Some(ref t_calls) = m.tool_calls {
                    for c in t_calls {
                        call_id_to_name.insert(c.id.clone(), c.name.clone());
                        let sig = c.thought_signature.as_deref().or(m.thought_signature.as_deref());
                        let mut fc = extract_raw_function_call(None, None, &c.name, &c.arguments, sig);
                        if let Some(s) = sig {
                            fc["thought_signature"] = json!(s);
                            fc["thoughtSignature"] = json!(s);
                        }
                        let mut part = json!({ "functionCall": fc });
                        if let Some(s) = sig {
                            part["thought_signature"] = json!(s);
                            part["thoughtSignature"] = json!(s);
                        }
                        parts.push(part);
                    }
                }
                if parts.is_empty() {
                    parts.push(json!({ "text": "" }));
                }
                contents.push(json!({
                    "role": "model",
                    "parts": parts
                }));
            }
            MessageRole::Tool => {
                let tool_name = m
                    .tool_call_id
                    .as_ref()
                    .and_then(|id| call_id_to_name.get(id))
                    .cloned()
                    .unwrap_or_else(|| "tool_result".to_string());

                let resp_val: serde_json::Value = serde_json::from_str(&m.content)
                    .unwrap_or_else(|_| json!({ "output": m.content }));

                contents.push(json!({
                    "role": "user",
                    "parts": [
                        {
                            "functionResponse": {
                                "name": tool_name,
                                "response": resp_val
                            }
                        }
                    ]
                }));
            }
        }
    }

    let mut payload = json!({
        "contents": contents
    });

    if !system_parts.is_empty() {
        payload["systemInstruction"] = json!({ "parts": system_parts });
    }

    if !req.tools.is_empty() {
        let decls: Vec<serde_json::Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                })
            })
            .collect();
        payload["tools"] = json!([{ "functionDeclarations": decls }]);
    }

    payload
}

pub fn parse_gemini_native_response(
    parsed: &serde_json::Value,
) -> Result<CompletionResponse, ProviderError> {
    if let Some(err) = parsed.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Gemini API error");
        return Err(ProviderError::Api {
            code: err.get("code").and_then(|c| c.as_u64()).unwrap_or(400) as u16,
            message: msg.to_string(),
        });
    }

    let candidates = parsed.get("candidates").and_then(|c| c.as_array());
    let candidate = match candidates.and_then(|c| c.first()) {
        Some(c) => c,
        None => {
            return Err(ProviderError::Api {
                code: 200,
                message: "No candidates returned from Gemini".to_string(),
            })
        }
    };

    let finish_reason = candidate
        .get("finishReason")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());
    let content_obj = candidate.get("content");
    let parts = content_obj.and_then(|c| c.get("parts")).and_then(|p| p.as_array());

    let mut text_content = String::new();
    let mut calls = Vec::new();

    let mut msg_thought_sig = find_thought_signature(None, None, content_obj, Some(candidate), Some(parsed));

    if let Some(parts_list) = parts {
        for (idx, part) in parts_list.iter().enumerate() {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                text_content.push_str(text);
            }
            if let Some(fc) = part.get("functionCall").or_else(|| part.get("function_call")) {
                if let Some(name) = fc.get("name").and_then(|n| n.as_str()) {
                    let arguments = fc
                        .get("args")
                        .or_else(|| fc.get("arguments"))
                        .cloned()
                        .unwrap_or(json!({}));
                    let sig = find_thought_signature(
                        Some(fc),
                        Some(part),
                        content_obj,
                        Some(candidate),
                        Some(parsed),
                    );
                    if msg_thought_sig.is_none() && sig.is_some() {
                        msg_thought_sig = sig.clone();
                    }
                    let raw_fc = extract_raw_function_call(
                        Some(fc),
                        Some(part),
                        name,
                        &arguments,
                        sig.as_deref(),
                    );
                    let id = format!("call_{}", idx);
                    calls.push(ToolCall {
                        id,
                        name: name.to_string(),
                        arguments,
                        thought_signature: sig,
                        raw_function_call: Some(raw_fc),
                    });
                }
            }
        }
    }

    let tool_calls = if calls.is_empty() { None } else { Some(calls) };

    Ok(CompletionResponse {
        message: ChatMessage {
            role: MessageRole::Assistant,
            content: text_content,
            images: None,
            tool_call_id: None,
            tool_calls,
            thought_signature: msg_thought_sig,
        },
        finish_reason,
    })
}

pub struct GeminiLlmProvider {
    inner: OpenAiLlmProvider,
    base_url: String,
    api_key: Option<String>,
    default_model: String,
}

impl GeminiLlmProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_model: impl Into<String>,
    ) -> Self {
        let default_model = default_model.into();
        let base_url = base_url.into();
        Self {
            inner: OpenAiLlmProvider::new(base_url.clone(), api_key.clone(), default_model.clone()),
            base_url,
            api_key,
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
        let model = if req.model.is_empty() || req.model == "default" {
            &self.default_model
        } else {
            &req.model
        };

        if self.base_url.contains("generativelanguage.googleapis.com") {
            let endpoint = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                model
            );
            let payload = build_gemini_native_payload(&req);
            let body_str = payload.to_string();
            let api_key = self.api_key.clone();

            let response_body = tokio::task::spawn_blocking(move || -> Result<String, ProviderError> {
                use std::io::Write;
                let curl_bin = if cfg!(target_os = "windows") { "curl.exe" } else { "curl" };
                let mut cmd = Command::new(curl_bin);
                let url = if let Some(ref key) = api_key {
                    if !key.is_empty() {
                        format!("{}?key={}", endpoint, key)
                    } else {
                        endpoint
                    }
                } else {
                    endpoint
                };

                cmd.arg("-s")
                    .arg("-X")
                    .arg("POST")
                    .arg(&url)
                    .arg("-H")
                    .arg("Content-Type: application/json");

                if let Some(ref key) = api_key {
                    if !key.is_empty() {
                        cmd.arg("-H").arg(format!("x-goog-api-key: {}", key));
                    }
                }

                cmd.arg("--data-binary")
                    .arg("@-")
                    .arg("-w")
                    .arg(format!("{}%{{http_code}}", CURL_STATUS_MARKER));
                cmd.stdin(std::process::Stdio::piped());
                cmd.stdout(std::process::Stdio::piped());
                cmd.stderr(std::process::Stdio::piped());

                let mut child = cmd.spawn().map_err(|e| ProviderError::Network(e.to_string()))?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(body_str.as_bytes()).map_err(|e| ProviderError::Network(e.to_string()))?;
                }
                let output = child.wait_with_output().map_err(|e| ProviderError::Network(e.to_string()))?;
                if !output.status.success() {
                    return Err(ProviderError::Network(format!("curl exited with code {:?}", output.status.code())));
                }
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            })
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))??;

            let (response_body, http_status) = split_curl_response(response_body);
            let status_code = http_status.unwrap_or(200);

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response_body) {
                if status_code < 400 && parsed.get("candidates").is_some() {
                    return parse_gemini_native_response(&parsed);
                }
            }
        }

        self.inner.complete(req).await
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

    #[test]
    fn test_gemini_regression_flow_thought_signature_preserved_across_turns() {
        let req = CompletionRequest {
            model: "gemini-2.5-flash".to_string(),
            messages: vec![
                ChatMessage::user("Open Chrome"),
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: "".to_string(),
                    images: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_open_app_0".to_string(),
                        name: "open_app".to_string(),
                        arguments: json!({ "name": "Google Chrome" }),
                        thought_signature: Some("sig_gemini_thinking_12345".to_string()),
                        raw_function_call: Some(json!({
                            "name": "open_app",
                            "args": { "name": "Google Chrome" },
                            "thought_signature": "sig_gemini_thinking_12345"
                        })),
                    }]),
                    thought_signature: Some("sig_gemini_thinking_12345".to_string()),
                },
                ChatMessage::tool("call_open_app_0", "{\"status\": \"opened\"}"),
            ],
            tools: vec![],
            temperature: None,
        };

        let payload = build_gemini_native_payload(&req);
        let contents = payload["contents"].as_array().unwrap();

        // Check user message turn
        assert_eq!(contents[0]["role"], "user");

        // Check model turn (Assistant with functionCall)
        let model_turn = &contents[1];
        assert_eq!(model_turn["role"], "model");
        let fc = &model_turn["parts"][0]["functionCall"];
        assert_eq!(fc["name"], "open_app");
        assert_eq!(fc["args"]["name"], "Google Chrome");
        assert_eq!(fc["thought_signature"], "sig_gemini_thinking_12345");
        assert_eq!(model_turn["parts"][0]["thought_signature"], "sig_gemini_thinking_12345");

        // Check functionResponse turn
        let tool_turn = &contents[2];
        assert_eq!(tool_turn["role"], "user");
        let fr = &tool_turn["parts"][0]["functionResponse"];
        assert_eq!(fr["name"], "open_app");
        assert_eq!(fr["response"]["status"], "opened");
    }

    #[test]
    fn test_gemini_multiple_consecutive_and_parallel_tool_calls_thought_signature() {
        let req = CompletionRequest {
            model: "gemini-2.5-flash".to_string(),
            messages: vec![
                ChatMessage::user("Open apps and take screenshot"),
                // Parallel tool calls in Assistant turn 1
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: "".to_string(),
                    images: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![
                        ToolCall {
                            id: "call_0".to_string(),
                            name: "open_app".to_string(),
                            arguments: json!({ "name": "Chrome" }),
                            thought_signature: Some("sig_parallel_1".to_string()),
                            raw_function_call: None,
                        },
                        ToolCall {
                            id: "call_1".to_string(),
                            name: "open_app".to_string(),
                            arguments: json!({ "name": "Terminal" }),
                            thought_signature: Some("sig_parallel_2".to_string()),
                            raw_function_call: None,
                        },
                    ]),
                    thought_signature: Some("sig_parallel_1".to_string()),
                },
                ChatMessage::tool("call_0", "{\"status\": \"opened Chrome\"}"),
                ChatMessage::tool("call_1", "{\"status\": \"opened Terminal\"}"),
                // Sequential tool call in Assistant turn 2
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: "".to_string(),
                    images: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_2".to_string(),
                        name: "take_screenshot".to_string(),
                        arguments: json!({}),
                        thought_signature: Some("sig_sequential_3".to_string()),
                        raw_function_call: None,
                    }]),
                    thought_signature: Some("sig_sequential_3".to_string()),
                },
                ChatMessage::tool("call_2", "{\"status\": \"captured\"}"),
            ],
            tools: vec![],
            temperature: None,
        };

        let payload = build_gemini_native_payload(&req);
        let contents = payload["contents"].as_array().unwrap();

        // Assistant turn 1 (Parallel calls)
        let model_turn_1 = &contents[1];
        assert_eq!(model_turn_1["parts"].as_array().unwrap().len(), 2);
        assert_eq!(model_turn_1["parts"][0]["functionCall"]["thought_signature"], "sig_parallel_1");
        assert_eq!(model_turn_1["parts"][1]["functionCall"]["thought_signature"], "sig_parallel_2");

        // Tool responses for turn 1
        assert_eq!(contents[2]["parts"][0]["functionResponse"]["name"], "open_app");
        assert_eq!(contents[3]["parts"][0]["functionResponse"]["name"], "open_app");

        // Assistant turn 2 (Sequential call)
        let model_turn_2 = &contents[4];
        assert_eq!(model_turn_2["parts"][0]["functionCall"]["name"], "take_screenshot");
        assert_eq!(model_turn_2["parts"][0]["functionCall"]["thought_signature"], "sig_sequential_3");

        // Tool response for turn 2
        assert_eq!(contents[5]["parts"][0]["functionResponse"]["name"], "take_screenshot");
    }

    #[test]
    fn test_parse_gemini_native_response_extracts_signatures_and_calls() {
        let raw_json = json!({
            "candidates": [
                {
                    "finishReason": "STOP",
                    "content": {
                        "role": "model",
                        "parts": [
                            {
                                "functionCall": {
                                    "name": "open_app",
                                    "args": { "name": "Google Chrome" },
                                    "thought_signature": "sig_native_response_999"
                                },
                                "thoughtSignature": "sig_native_response_999"
                            }
                        ]
                    }
                }
            ]
        });

        let resp = parse_gemini_native_response(&raw_json).unwrap();
        assert_eq!(resp.message.role, MessageRole::Assistant);
        assert_eq!(resp.message.thought_signature, Some("sig_native_response_999".to_string()));

        let tool_calls = resp.message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "open_app");
        assert_eq!(tool_calls[0].thought_signature, Some("sig_native_response_999".to_string()));
        assert!(tool_calls[0].raw_function_call.is_some());
    }
}

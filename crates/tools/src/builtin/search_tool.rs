use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

/// Tool for live web search queries.
pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for technical documentation, solutions, error messages, and general information."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search keywords or query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 5)"
                }
            },
            "required": ["query"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'query' parameter".into(),
            })?;

        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let q = query.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let encoded = urlencoding_simple(&q);
            let url = format!(
                "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
                encoded
            );

            let curl_bin = if cfg!(target_os = "windows") {
                "curl.exe"
            } else {
                "curl"
            };
            let mut cmd = Command::new(curl_bin);
            cmd.arg("-s")
                .arg("-L")
                .arg("--max-time")
                .arg("8")
                .arg("-A")
                .arg("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                .arg(&url);

            let output = cmd.output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!("curl error: code {:?}", output.status.code()));
            }

            let body = String::from_utf8_lossy(&output.stdout).to_string();
            let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));

            let mut items = Vec::new();

            if let Some(abs_text) = parsed.get("AbstractText").and_then(|t| t.as_str()) {
                if !abs_text.is_empty() {
                    let heading = parsed.get("Heading").and_then(|h| h.as_str()).unwrap_or(&q);
                    let abs_url = parsed
                        .get("AbstractURL")
                        .and_then(|u| u.as_str())
                        .unwrap_or("");
                    items.push(json!({
                        "title": heading,
                        "url": abs_url,
                        "snippet": abs_text
                    }));
                }
            }

            if let Some(topics) = parsed.get("RelatedTopics").and_then(|t| t.as_array()) {
                for item in topics {
                    if items.len() >= max_results {
                        break;
                    }
                    if let Some(text) = item.get("Text").and_then(|t| t.as_str()) {
                        let first_url = item.get("FirstURL").and_then(|u| u.as_str()).unwrap_or("");
                        items.push(json!({
                            "title": text.chars().take(60).collect::<String>(),
                            "url": first_url,
                            "snippet": text
                        }));
                    }
                }
            }

            Ok(items)
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            tool: self.name().into(),
            details: e.to_string(),
        })?;

        match result {
            Ok(items) => {
                let count = items.len();
                Ok(ToolResult::success(
                    format!("Found {} search results for \"{}\"", count, query),
                    json!({ "query": query, "results": items }),
                ))
            }
            Err(e) => Ok(ToolResult::failure(
                format!("Search query failed for \"{}\"", query),
                e,
            )),
        }
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            result.push(b as char);
        } else if b == b' ' {
            result.push('+');
        } else {
            result.push_str(&format!("%{:02X}", b));
        }
    }
    result
}

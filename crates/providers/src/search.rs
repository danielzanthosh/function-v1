use crate::{ProviderError, SearchProvider, SearchResult};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

pub struct DuckDuckGoSearchProvider;

impl DuckDuckGoSearchProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DuckDuckGoSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for DuckDuckGoSearchProvider {
    fn name(&self) -> &str {
        "duckduckgo"
    }

    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        let q = query.to_string();
        tokio::task::spawn_blocking(move || -> Result<Vec<SearchResult>, ProviderError> {
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
                .arg("-A")
                .arg("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                .arg(&url);

            let output = cmd
                .output()
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            if !output.status.success() {
                return Ok(Vec::new());
            }

            let body = String::from_utf8_lossy(&output.stdout).to_string();
            let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));

            let mut results = Vec::new();

            // Check Heading / Abstract
            if let Some(abs_text) = parsed.get("AbstractText").and_then(|t| t.as_str()) {
                if !abs_text.is_empty() {
                    let heading = parsed.get("Heading").and_then(|h| h.as_str()).unwrap_or(&q);
                    let abs_url = parsed
                        .get("AbstractURL")
                        .and_then(|u| u.as_str())
                        .unwrap_or("");
                    results.push(SearchResult {
                        title: heading.to_string(),
                        url: abs_url.to_string(),
                        snippet: abs_text.to_string(),
                    });
                }
            }

            // Related topics
            if let Some(topics) = parsed.get("RelatedTopics").and_then(|t| t.as_array()) {
                for item in topics {
                    if results.len() >= max_results {
                        break;
                    }
                    if let Some(text) = item.get("Text").and_then(|t| t.as_str()) {
                        let first_url = item.get("FirstURL").and_then(|u| u.as_str()).unwrap_or("");
                        results.push(SearchResult {
                            title: text.chars().take(60).collect::<String>(),
                            url: first_url.to_string(),
                            snippet: text.to_string(),
                        });
                    }
                }
            }

            Ok(results)
        })
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?
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

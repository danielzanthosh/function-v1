use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

/// Tool for native browser control and web page inspection.
pub struct BrowserTool;

impl BrowserTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BrowserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser_control"
    }

    fn description(&self) -> &str {
        "Open URLs in the user's default browser or fetch web page text for inspection."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["open", "fetch"],
                    "description": "The browser action: 'open' to launch in browser, or 'fetch' to read page text"
                },
                "url": {
                    "type": "string",
                    "description": "The URL to navigate to or fetch"
                }
            },
            "required": ["action", "url"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = params.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'action' parameter".into(),
            }
        })?;

        let url = params.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'url' parameter".into(),
            }
        })?;

        let safe_url = if !url.starts_with("http://") && !url.starts_with("https://") {
            format!("https://{}", url)
        } else {
            url.to_string()
        };

        match action {
            "open" | "navigate" => {
                function_platform::open_url(&safe_url);
                Ok(ToolResult::success(
                    format!("Opened URL: {}", safe_url),
                    json!({ "status": "opened", "url": safe_url }),
                ))
            }
            "fetch" | "read" => {
                let fetch_url = safe_url.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let curl_bin = if cfg!(target_os = "windows") { "curl.exe" } else { "curl" };
                    let mut cmd = Command::new(curl_bin);
                    cmd.arg("-s")
                        .arg("-L")
                        .arg("--max-time")
                        .arg("10")
                        .arg("-A")
                        .arg("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                        .arg(&fetch_url);

                    let output = cmd.output().map_err(|e| e.to_string())?;
                    if !output.status.success() {
                        return Err(format!("curl exited with code {:?}", output.status.code()));
                    }

                    let raw_html = String::from_utf8_lossy(&output.stdout).to_string();
                    let clean_text = strip_html_tags(&raw_html);
                    let truncated: String = clean_text.chars().take(4000).collect();
                    Ok(truncated)
                })
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;

                match result {
                    Ok(text) => Ok(ToolResult::success(
                        format!("Fetched content from {}", safe_url),
                        json!({ "url": safe_url, "content": text }),
                    )),
                    Err(e) => Ok(ToolResult::failure(
                        format!("Failed to fetch {}", safe_url),
                        e,
                    )),
                }
            }
            unknown => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown browser action '{}'", unknown),
            }),
        }
    }
}

/// Simple regex-free HTML tag stripper to produce legible text extracts.
fn strip_html_tags(html: &str) -> String {
    let mut in_tag = false;
    let mut in_script = false;
    let mut result = String::with_capacity(html.len() / 2);
    let mut word_boundary = false;

    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if in_script {
            if chars[i..].starts_with(&['<', '/', 's', 'c', 'r', 'i', 'p', 't', '>'])
                || chars[i..].starts_with(&['<', '/', 'S', 'C', 'R', 'I', 'P', 'T', '>'])
                || chars[i..].starts_with(&['<', '/', 's', 't', 'y', 'l', 'e', '>'])
                || chars[i..].starts_with(&['<', '/', 'S', 'T', 'Y', 'L', 'E', '>'])
            {
                in_script = false;
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        if chars[i] == '<' {
            in_tag = true;
            if chars[i..].starts_with(&['<', 's', 'c', 'r', 'i', 'p', 't'])
                || chars[i..].starts_with(&['<', 'S', 'C', 'R', 'I', 'P', 'T'])
                || chars[i..].starts_with(&['<', 's', 't', 'y', 'l', 'e'])
                || chars[i..].starts_with(&['<', 'S', 'T', 'Y', 'L', 'E'])
            {
                in_script = true;
            }
            i += 1;
            continue;
        }

        if chars[i] == '>' {
            in_tag = false;
            if !word_boundary {
                result.push(' ');
                word_boundary = true;
            }
            i += 1;
            continue;
        }

        if !in_tag {
            let ch = chars[i];
            if ch.is_whitespace() {
                if !word_boundary {
                    result.push(' ');
                    word_boundary = true;
                }
            } else {
                result.push(ch);
                word_boundary = false;
            }
        }
        i += 1;
    }

    result.trim().to_string()
}

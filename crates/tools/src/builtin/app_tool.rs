use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use function_platform::{create_computer_control, ComputerControl};
use serde_json::json;
use std::sync::Arc;

pub struct ApplicationTool {
    control: Arc<dyn ComputerControl>,
}

impl ApplicationTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for ApplicationTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ApplicationTool {
    fn name(&self) -> &str {
        "computer_apps"
    }

    fn description(&self) -> &str {
        "Launch applications, list running visible windows, or focus a specific window by title."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["launch", "list_windows", "focus_window"]
                },
                "app": { "type": "string", "description": "Application executable name or command, e.g. 'notepad', 'chrome', 'calc'" },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional command line arguments"
                },
                "window_title": { "type": "string", "description": "Title or substring of window to focus" }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'action' parameter".into(),
            })?;

        match action {
            "launch" => {
                let app = params.get("app").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'app' parameter".into(),
                    }
                })?;
                let empty_args = Vec::new();
                let args_json = params.get("args").and_then(|v| v.as_array());
                let args: Vec<&str> = if let Some(arr) = args_json {
                    arr.iter().filter_map(|v| v.as_str()).collect()
                } else {
                    empty_args
                };

                let pid = self.control.app_launch(app, &args).map_err(|e| {
                    ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    }
                })?;

                Ok(ToolResult::success(
                    format!("Launched application '{}' (PID: {})", app, pid),
                    json!({ "app": app, "pid": pid }),
                ))
            }
            "list_windows" => {
                let windows = self.control.list_windows();
                let count = windows.len();
                Ok(ToolResult::success(
                    format!("Found {} visible windows", count),
                    json!({ "windows": windows }),
                ))
            }
            "focus_window" => {
                let title = params
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'window_title' parameter".into(),
                    })?;

                let focused =
                    self.control
                        .focus_window(title)
                        .map_err(|e| ToolError::ExecutionFailed {
                            tool: self.name().into(),
                            details: e.to_string(),
                        })?;

                if focused {
                    Ok(ToolResult::success(
                        format!("Focused window matching '{}'", title),
                        json!({ "focused": true, "query": title }),
                    ))
                } else {
                    Ok(ToolResult::failure(
                        format!("Window matching '{}' not found", title),
                        "No matching visible window found",
                    ))
                }
            }
            other => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown application action: '{}'", other),
            }),
        }
    }
}

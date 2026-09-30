use crate::{Tool, ToolContext, ToolError, ToolResult};
use assistant_platform::{create_computer_control, ComputerControl};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct KeyboardTool {
    control: Arc<dyn ComputerControl>,
}

impl KeyboardTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for KeyboardTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for KeyboardTool {
    fn name(&self) -> &str {
        "computer_keyboard"
    }

    fn description(&self) -> &str {
        "Type text or press keyboard shortcuts (e.g. ['ctrl', 'c'], Enter, Esc)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["type", "press", "shortcut"]
                },
                "text": { "type": "string", "description": "Text to type" },
                "key": { "type": "string", "description": "Key to press (e.g. 'enter', 'tab', 'esc', 'space')" },
                "keys": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Combination of keys for a shortcut, e.g. ['ctrl', 'shift', 'p']"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = params.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'action' parameter".into(),
            }
        })?;

        match action {
            "type" => {
                let text = params.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'text' parameter".into(),
                    }
                })?;
                self.control.keyboard_type(text).map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                Ok(ToolResult::success(format!("Typed {} characters", text.len()), json!({ "typed_length": text.len() })))
            }
            "press" => {
                let key = params.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'key' parameter".into(),
                    }
                })?;
                self.control.keyboard_press(key).map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                Ok(ToolResult::success(format!("Pressed key '{}'", key), json!({ "key": key })))
            }
            "shortcut" => {
                let keys_arr = params.get("keys").and_then(|v| v.as_array()).ok_or_else(|| {
                    ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'keys' array parameter".into(),
                    }
                })?;
                let keys: Vec<&str> = keys_arr.iter().filter_map(|k| k.as_str()).collect();
                self.control.keyboard_shortcut(&keys).map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                let combo = keys.join("+");
                Ok(ToolResult::success(format!("Sent shortcut '{}'", combo), json!({ "shortcut": combo })))
            }
            other => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown keyboard action: '{}'", other),
            }),
        }
    }
}

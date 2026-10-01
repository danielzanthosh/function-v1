use crate::{Tool, ToolContext, ToolError, ToolResult};
use function_platform::{create_computer_control, ComputerControl};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct ScreenTool {
    control: Arc<dyn ComputerControl>,
}

impl ScreenTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for ScreenTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ScreenTool {
    fn name(&self) -> &str {
        "computer_screen"
    }

    fn description(&self) -> &str {
        "Inspect screen resolution and mouse cursor coordinates."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["dimensions", "cursor_position"]
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("dimensions");

        match action {
            "dimensions" => {
                let dims = self.control.get_screen_dimensions();
                Ok(ToolResult::success(
                    format!("Screen resolution: {}x{}", dims.width, dims.height),
                    json!({ "width": dims.width, "height": dims.height }),
                ))
            }
            "cursor_position" => {
                let (x, y) = self.control.get_cursor_position();
                Ok(ToolResult::success(
                    format!("Cursor position: ({}, {})", x, y),
                    json!({ "cursor_x": x, "cursor_y": y }),
                ))
            }
            other => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown screen action: '{}'", other),
            }),
        }
    }
}

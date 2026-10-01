use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use function_platform::{create_computer_control, ComputerControl, MouseButton};
use serde_json::json;
use std::sync::Arc;

pub struct MouseTool {
    control: Arc<dyn ComputerControl>,
}

impl MouseTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for MouseTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MouseTool {
    fn name(&self) -> &str {
        "computer_mouse"
    }

    fn description(&self) -> &str {
        "Control mouse cursor: move to coordinates, click, double click, right click, scroll, or drag."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["move", "click", "double_click", "right_click", "scroll", "drag"]
                },
                "x": { "type": "integer", "description": "Target X pixel coordinate" },
                "y": { "type": "integer", "description": "Target Y pixel coordinate" },
                "delta": { "type": "integer", "description": "Scroll delta (positive for up, negative for down)" },
                "start_x": { "type": "integer", "description": "Drag starting X coordinate" },
                "start_y": { "type": "integer", "description": "Drag starting Y coordinate" },
                "end_x": { "type": "integer", "description": "Drag ending X coordinate" },
                "end_y": { "type": "integer", "description": "Drag ending Y coordinate" }
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
            "move" => {
                let x = params.get("x").and_then(|v| v.as_i64()).ok_or_else(|| {
                    ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'x' coordinate".into(),
                    }
                })? as i32;
                let y = params.get("y").and_then(|v| v.as_i64()).ok_or_else(|| {
                    ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'y' coordinate".into(),
                    }
                })? as i32;

                self.control
                    .mouse_move(x, y)
                    .map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    })?;
                Ok(ToolResult::success(
                    format!("Moved mouse to ({}, {})", x, y),
                    json!({ "x": x, "y": y }),
                ))
            }
            "click" => {
                self.control.mouse_click(MouseButton::Left).map_err(|e| {
                    ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    }
                })?;
                Ok(ToolResult::success(
                    "Mouse clicked",
                    json!({ "button": "left" }),
                ))
            }
            "double_click" => {
                self.control
                    .mouse_double_click(MouseButton::Left)
                    .map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    })?;
                Ok(ToolResult::success(
                    "Mouse double clicked",
                    json!({ "button": "left" }),
                ))
            }
            "right_click" => {
                self.control.mouse_click(MouseButton::Right).map_err(|e| {
                    ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    }
                })?;
                Ok(ToolResult::success(
                    "Mouse right clicked",
                    json!({ "button": "right" }),
                ))
            }
            "scroll" => {
                let delta = params.get("delta").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                self.control
                    .mouse_scroll(delta)
                    .map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    })?;
                Ok(ToolResult::success(
                    format!("Mouse scrolled by {}", delta),
                    json!({ "delta": delta }),
                ))
            }
            "drag" => {
                let sx = params.get("start_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let sy = params.get("start_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let ex = params.get("end_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let ey = params.get("end_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                self.control.mouse_drag(sx, sy, ex, ey).map_err(|e| {
                    ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    }
                })?;
                Ok(ToolResult::success(
                    format!("Dragged mouse from ({}, {}) to ({}, {})", sx, sy, ex, ey),
                    json!({ "start": [sx, sy], "end": [ex, ey] }),
                ))
            }
            other => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown mouse action: '{}'", other),
            }),
        }
    }
}

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
                    "enum": ["move", "click", "double_click", "right_click", "middle_click", "scroll", "drag"]
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
                    .mouse_move_smooth(x, y)
                    .map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: e.to_string(),
                    })?;
                Ok(ToolResult::success(
                    format!("Moved mouse to ({}, {})", x, y),
                    json!({ "x": x, "y": y }),
                ))
            }
            "click" | "double_click" | "right_click" | "middle_click" => {
                // The model may provide coordinates for a click action. Previously these
                // actions ignored x/y and clicked wherever the cursor happened to be.
                if let (Some(x), Some(y)) = (
                    params.get("x").and_then(|v| v.as_i64()),
                    params.get("y").and_then(|v| v.as_i64()),
                ) {
                    self.control
                        .mouse_move_smooth(x as i32, y as i32)
                        .map_err(|e| ToolError::ExecutionFailed {
                            tool: self.name().into(),
                            details: format!("Failed to move to click target: {}", e),
                        })?;
                }

                let button = match action {
                    "right_click" => MouseButton::Right,
                    "middle_click" => MouseButton::Middle,
                    _ => MouseButton::Left,
                };

                match action {
                    "double_click" => self.control.mouse_double_click(button),
                    _ => self.control.mouse_click(button),
                }
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;

                let (x, y) = self.control.get_cursor_position();
                Ok(ToolResult::success(
                    format!("{} at ({}, {})", action.replace('_', " "), x, y),
                    json!({ "action": action, "x": x, "y": y, "button": format!("{:?}", button).to_lowercase() }),
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

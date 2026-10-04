use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use function_platform::{create_computer_control, ComputerControl};
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
        "Inspect screen resolution, mouse cursor coordinates, or capture a screen screenshot."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["dimensions", "cursor_position", "screenshot"],
                    "description": "The screen action: 'screenshot' to capture visual screen content, 'dimensions' for screen resolution, or 'cursor_position' for coordinates."
                }
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
            .unwrap_or("screenshot");

        match action {
            "screenshot" => {
                let dims = self.control.get_screen_dimensions();
                match self.control.take_screenshot() {
                    Ok(bytes) => {
                        let b64 = to_base64(&bytes);

                        let home = std::env::var_os("USERPROFILE")
                            .or_else(|| std::env::var_os("HOME"))
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| std::path::PathBuf::from("."));
                        let mut dir = home;
                        dir.push(".function");
                        dir.push("attachments");
                        let _ = std::fs::create_dir_all(&dir);

                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        let path = dir.join(format!("screenshot_{}.png", ts));
                        let path_str = path.display().to_string();
                        let _ = std::fs::write(&path, &bytes);

                        let grid_cols = 10;
                        let grid_rows = 10;
                        let cell_w = dims.logical_width as f64 / grid_cols as f64;
                        let cell_h = dims.logical_height as f64 / grid_rows as f64;

                        Ok(ToolResult::success(
                            format!(
                                "Screenshot captured (Physical: {}x{}, Logical: {}x{}, Scale: {}x)",
                                dims.width, dims.height, dims.logical_width, dims.logical_height, dims.scale_factor
                            ),
                            json!({
                                "status": "success",
                                "physical_width": dims.width,
                                "physical_height": dims.height,
                                "logical_width": dims.logical_width,
                                "logical_height": dims.logical_height,
                                "scale_factor": dims.scale_factor,
                                "virtual_grid": {
                                    "columns": grid_cols,
                                    "rows": grid_rows,
                                    "cell_width_logical": cell_w,
                                    "cell_height_logical": cell_h,
                                },
                                "path": path_str,
                                "base64": b64,
                            }),
                        ))
                    }
                    Err(e) => Ok(ToolResult::failure(
                        "Failed to capture screen",
                        format!("{}", e),
                    )),
                }
            }
            "dimensions" => {
                let dims = self.control.get_screen_dimensions();
                Ok(ToolResult::success(
                    format!("Screen resolution: Physical {}x{}, Logical {}x{} (Scale: {}x)", dims.width, dims.height, dims.logical_width, dims.logical_height, dims.scale_factor),
                    json!({
                        "physical_width": dims.width,
                        "physical_height": dims.height,
                        "logical_width": dims.logical_width,
                        "logical_height": dims.logical_height,
                        "scale_factor": dims.scale_factor,
                    }),
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

fn to_base64(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARSET[(triple >> 18) & 0x3F] as char);
        result.push(CHARSET[(triple >> 12) & 0x3F] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[(triple >> 6) & 0x3F] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[triple & 0x3F] as char);
        } else {
            result.push('=');
        }
    }
    result
}

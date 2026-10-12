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

                        // Screenshot bytes remain in memory and are passed directly to the model.
                        // Do not persist temporary visual context in the user's attachments folder.
                        Ok(ToolResult::success(
                            format!(
                                "Screenshot captured successfully ({}x{} image pixels; screen input space {}x{} logical points; {} KB)",
                                png_dimensions(&bytes).map(|d| d.0).unwrap_or(dims.width),
                                png_dimensions(&bytes).map(|d| d.1).unwrap_or(dims.height),
                                dims.width,
                                dims.height,
                                bytes.len() / 1024
                            ),
                            {
                                let (pixel_width, pixel_height) = png_dimensions(&bytes).unwrap_or((dims.width, dims.height));
                                json!({
                                    "status": "success",
                                    "width": pixel_width,
                                    "height": pixel_height,
                                    "pixel_width": pixel_width,
                                    "pixel_height": pixel_height,
                                    "screen_width": dims.width,
                                    "screen_height": dims.height,
                                    "coordinate_space": "pixel coordinates in returned screenshot image; mouse tool converts once to logical screen points",
                                    "base64": b64,
                                })
                            },
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

fn png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 24 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        if width > 0 && height > 0 { return Some((width, height)); }
    }
    None
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

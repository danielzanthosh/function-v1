//! High-level native desktop control tools.
//!
//! Provides OS-aware capabilities:
//! - `open_app`
//! - `close_app`
//! - `take_screenshot`
//! - `click`
//! - `double_click`
//! - `type_text`
//! - `press_key`
//! - `scroll`
//! - `execute_command`

use crate::{Tool, ToolContext, ToolError, ToolPermissionLevel, ToolResult};
use async_trait::async_trait;
use function_platform::{create_computer_control, ComputerControl, MouseButton};
use serde_json::json;
use std::process::Command;
use std::sync::Arc;

/// Native OS-aware application opener.
pub struct OpenAppTool {
    control: Arc<dyn ComputerControl>,
}

impl OpenAppTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for OpenAppTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for OpenAppTool {
    fn name(&self) -> &str {
        "open_app"
    }

    fn description(&self) -> &str {
        "Launch or focus a desktop application by name (e.g. 'Google Chrome', 'Slack', 'VS Code', 'Terminal', 'Calculator', 'Notes', 'Finder'). Uses native OS application APIs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Common name or executable name of the application, e.g. 'Google Chrome' or 'Terminal'"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional command-line arguments or URL to pass to the application"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'name' parameter".into(),
            }
        })?;

        let empty_args = Vec::new();
        let args_json = params.get("args").and_then(|v| v.as_array());
        let args: Vec<&str> = if let Some(arr) = args_json {
            arr.iter().filter_map(|v| v.as_str()).collect()
        } else {
            empty_args
        };

        // Try standard computer control launch
        let launch_result = self.control.app_launch(name, &args);
        match launch_result {
            Ok(pid) => Ok(ToolResult::success(
                format!("Opened application '{}' (PID: {})", name, pid),
                json!({
                    "success": true,
                    "app": name,
                    "pid": pid,
                    "platform": std::env::consts::OS,
                    "message": format!("Application '{}' launched successfully", name)
                }),
            )),
            Err(err) => {
                // Secondary fallback via system search
                let search_matches = function_platform::search_apps_and_files(name);
                if let Some(first_app) = search_matches
                    .iter()
                    .find(|m| m.kind == function_platform::SearchItemKind::Application)
                {
                    function_platform::open_path(&first_app.path);
                    return Ok(ToolResult::success(
                        format!("Opened application '{}' via system path", first_app.name),
                        json!({
                            "success": true,
                            "app": first_app.name,
                            "path": first_app.path,
                            "platform": std::env::consts::OS,
                            "message": format!("Application '{}' launched from path {}", first_app.name, first_app.path)
                        }),
                    ));
                }

                Ok(ToolResult::failure(
                    format!("Failed to open '{}'", name),
                    format!(
                        "Could not launch application '{}' on {}: {}. Tip: verify the application name or search with search_apps_and_files.",
                        name,
                        std::env::consts::OS,
                        err
                    ),
                ))
            }
        }
    }
}

/// Native application closer.
pub struct CloseAppTool;

impl CloseAppTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CloseAppTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CloseAppTool {
    fn name(&self) -> &str {
        "close_app"
    }

    fn description(&self) -> &str {
        "Close or quit a running desktop application by name."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the application to close, e.g. 'Google Chrome' or 'Calculator'"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'name' parameter".into(),
            }
        })?;

        #[cfg(target_os = "macos")]
        let res = {
            let script = format!("tell application \"{}\" to quit", name.replace('"', "\\\""));
            Command::new("osascript").args(["-e", &script]).output()
        };

        #[cfg(target_os = "windows")]
        let res = {
            let proc_name = if name.to_lowercase().ends_with(".exe") {
                name.to_string()
            } else {
                format!("{}.exe", name)
            };
            Command::new("taskkill")
                .args(["/IM", &proc_name, "/T"])
                .output()
        };

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let res = Command::new("pkill").arg("-f").arg(name).output();

        match res {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let success = output.status.success();
                if success {
                    Ok(ToolResult::success(
                        format!("Closed application '{}'", name),
                        json!({
                            "success": true,
                            "app": name,
                            "message": format!("Application '{}' closed successfully", name),
                            "stdout": stdout
                        }),
                    ))
                } else {
                    Ok(ToolResult::failure(
                        format!("Could not close '{}'", name),
                        format!("Close attempt returned error: {}", stderr.trim()),
                    ))
                }
            }
            Err(e) => Ok(ToolResult::failure(
                format!("Failed to close '{}'", name),
                e.to_string(),
            )),
        }
    }
}

/// Capture screen context and inject into vision models.
pub struct TakeScreenshotTool {
    control: Arc<dyn ComputerControl>,
}

impl TakeScreenshotTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for TakeScreenshotTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TakeScreenshotTool {
    fn name(&self) -> &str {
        "take_screenshot"
    }

    fn description(&self) -> &str {
        "Capture the current desktop screen and return high-resolution visual context for vision analysis, verifying UI state, reading display contents, or debugging errors."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let bytes = self
            .control
            .take_screenshot()
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().into(),
                details: e.to_string(),
            })?;

        let dims = self.control.get_screen_dimensions();

        // Save screenshot to disk for persistent reference
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let mut out_dir = home;
        out_dir.push(".function");
        out_dir.push("attachments");
        let _ = std::fs::create_dir_all(&out_dir);

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let file_path = out_dir.join(format!("screenshot_{}.png", ts));
        let _ = std::fs::write(&file_path, &bytes);

        // Standard RFC4648 Base64 encoding
        const B64_CHARS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::with_capacity((bytes.len() + 2) / 3 * 4);
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0];
            let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

            b64.push(B64_CHARS[(b0 >> 2) as usize] as char);
            b64.push(B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                b64.push(B64_CHARS[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                b64.push('=');
            }
            if chunk.len() > 2 {
                b64.push(B64_CHARS[(b2 & 0x3F) as usize] as char);
            } else {
                b64.push('=');
            }
        }

        Ok(ToolResult::success(
            format!("Captured screen ({}x{})", dims.width, dims.height),
            json!({
                "success": true,
                "width": dims.width,
                "height": dims.height,
                "path": file_path.to_string_lossy(),
                "base64": b64,
                "message": "Screenshot captured successfully"
            }),
        ))
    }
}

/// Mouse click tool.
pub struct ClickTool {
    control: Arc<dyn ComputerControl>,
}

impl ClickTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for ClickTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ClickTool {
    fn name(&self) -> &str {
        "click"
    }

    fn description(&self) -> &str {
        "Click the mouse at target screen coordinates (x, y) or at the current cursor position."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer", "description": "Target X screen pixel coordinate" },
                "y": { "type": "integer", "description": "Target Y screen pixel coordinate" },
                "button": {
                    "type": "string",
                    "enum": ["left", "right", "middle"],
                    "description": "Mouse button to click (default: 'left')"
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let x_opt = params.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
        let y_opt = params.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);

        if let (Some(x), Some(y)) = (x_opt, y_opt) {
            self.control
                .mouse_move_smooth(x, y)
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
        }

        let button_str = params
            .get("button")
            .and_then(|v| v.as_str())
            .unwrap_or("left");
        let btn = match button_str {
            "right" => MouseButton::Right,
            "middle" => MouseButton::Middle,
            _ => MouseButton::Left,
        };

        self.control
            .mouse_click(btn)
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().into(),
                details: e.to_string(),
            })?;

        let (cur_x, cur_y) = self.control.get_cursor_position();

        Ok(ToolResult::success(
            format!("Clicked {} button at ({}, {})", button_str, cur_x, cur_y),
            json!({
                "success": true,
                "x": cur_x,
                "y": cur_y,
                "button": button_str
            }),
        ))
    }
}

/// Mouse double-click tool.
pub struct DoubleClickTool {
    control: Arc<dyn ComputerControl>,
}

impl DoubleClickTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for DoubleClickTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for DoubleClickTool {
    fn name(&self) -> &str {
        "double_click"
    }

    fn description(&self) -> &str {
        "Double-click the mouse at target screen coordinates (x, y) or at the current cursor position."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer", "description": "Target X screen pixel coordinate" },
                "y": { "type": "integer", "description": "Target Y screen pixel coordinate" }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let x_opt = params.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
        let y_opt = params.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);

        if let (Some(x), Some(y)) = (x_opt, y_opt) {
            self.control
                .mouse_move_smooth(x, y)
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
        }

        self.control
            .mouse_double_click(MouseButton::Left)
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().into(),
                details: e.to_string(),
            })?;

        let (cur_x, cur_y) = self.control.get_cursor_position();

        Ok(ToolResult::success(
            format!("Double-clicked at ({}, {})", cur_x, cur_y),
            json!({
                "success": true,
                "x": cur_x,
                "y": cur_y
            }),
        ))
    }
}

/// Text typing tool.
pub struct TypeTextTool {
    control: Arc<dyn ComputerControl>,
}

impl TypeTextTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for TypeTextTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TypeTextTool {
    fn name(&self) -> &str {
        "type_text"
    }

    fn description(&self) -> &str {
        "Type unicode text into the currently focused window or text input field."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "The exact string of text to type" }
            },
            "required": ["text"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let text = params.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'text' parameter".into(),
            }
        })?;

        self.control
            .keyboard_type_smooth(text)
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().into(),
                details: e.to_string(),
            })?;

        Ok(ToolResult::success(
            format!("Typed {} characters", text.chars().count()),
            json!({
                "success": true,
                "length": text.chars().count()
            }),
        ))
    }
}

/// Key press & keyboard shortcut tool.
pub struct PressKeyTool {
    control: Arc<dyn ComputerControl>,
}

impl PressKeyTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for PressKeyTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PressKeyTool {
    fn name(&self) -> &str {
        "press_key"
    }

    fn description(&self) -> &str {
        "Press a keyboard key or hotkey combination (e.g. 'return', 'enter', 'tab', 'escape', 'space', 'up', 'down', 'cmd+t', 'ctrl+c', 'cmd+w', 'alt+f4')."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Key name or hotkey combination separated by '+' (e.g. 'return', 'cmd+t', 'ctrl+c')"
                }
            },
            "required": ["key"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let key = params.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'key' parameter".into(),
            }
        })?;

        if key.contains('+') {
            let parts: Vec<&str> = key.split('+').map(|s| s.trim()).collect();
            self.control
                .keyboard_shortcut(&parts)
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
        } else {
            self.control
                .keyboard_press(key)
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
        }

        Ok(ToolResult::success(
            format!("Pressed key '{}'", key),
            json!({
                "success": true,
                "key": key
            }),
        ))
    }
}

/// Mouse scroll tool.
pub struct ScrollTool {
    control: Arc<dyn ComputerControl>,
}

impl ScrollTool {
    pub fn new() -> Self {
        Self {
            control: Arc::from(create_computer_control()),
        }
    }
}

impl Default for ScrollTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ScrollTool {
    fn name(&self) -> &str {
        "scroll"
    }

    fn description(&self) -> &str {
        "Scroll the active window or page horizontally and vertically (positive delta_y scrolls down, negative scrolls up)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "delta_x": { "type": "integer", "description": "Horizontal scroll delta (default: 0)" },
                "delta_y": { "type": "integer", "description": "Vertical scroll delta (e.g. 5 to scroll down, -5 to scroll up)" }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let delta_x = params.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let delta_y = params.get("delta_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let delta = if delta_y != 0 { delta_y } else { delta_x };
        self.control
            .mouse_scroll(delta)
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().into(),
                details: e.to_string(),
            })?;

        Ok(ToolResult::success(
            format!("Scrolled (delta_x: {}, delta_y: {})", delta_x, delta_y),
            json!({
                "success": true,
                "delta_x": delta_x,
                "delta_y": delta_y
            }),
        ))
    }
}

/// Fallback shell command execution tool with smart confirmation.
pub struct ExecuteCommandTool;

impl ExecuteCommandTool {
    pub fn new() -> Self {
        Self
    }

    fn is_restricted(cmd: &str) -> bool {
        let lower = cmd.to_lowercase();
        lower.contains("format ")
            || lower.contains("diskpart")
            || lower.contains("rm -rf /")
            || lower.contains("rmdir /s /q c:\\")
            || lower.contains(":(){ :|:& };:")
    }

    fn is_destructive(cmd: &str) -> bool {
        let lower = cmd.to_lowercase();
        lower.contains("rmdir")
            || lower.contains("rm -rf")
            || lower.contains("del /f")
            || lower.contains("remove-item")
            || lower.contains("reg delete")
            || lower.contains("shutdown")
            || lower.contains("stop-computer")
            || lower.contains("drop database")
            || lower.contains("kill -9")
            || lower.contains("taskkill /f")
    }
}

impl Default for ExecuteCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ExecuteCommandTool {
    fn name(&self) -> &str {
        "execute_command"
    }

    fn description(&self) -> &str {
        "Execute a shell or terminal command as a fallback when native tools are insufficient."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command line string to execute" },
                "cwd": { "type": "string", "description": "Optional working directory" }
            },
            "required": ["command"]
        })
    }

    fn permission_level(&self, params: &serde_json::Value) -> ToolPermissionLevel {
        let cmd = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if Self::is_restricted(cmd) {
            ToolPermissionLevel::Restricted
        } else if Self::is_destructive(cmd) {
            ToolPermissionLevel::Confirm
        } else {
            ToolPermissionLevel::Safe
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'command' parameter".into(),
            })?;

        if Self::is_destructive(command) && !ctx.allow_sensitive {
            return Err(ToolError::RequiresConfirmation);
        }

        let cwd = params.get("cwd").and_then(|v| v.as_str());

        #[cfg(target_os = "windows")]
        let mut proc = {
            let mut c = Command::new("powershell.exe");
            c.arg("-NoProfile").arg("-Command").arg(command);
            c
        };

        #[cfg(not(target_os = "windows"))]
        let mut proc = {
            let mut c = Command::new("sh");
            c.arg("-c").arg(command);
            c
        };

        if let Some(dir) = cwd {
            proc.current_dir(dir);
        }

        let output = proc.output().map_err(|e| ToolError::ExecutionFailed {
            tool: self.name().into(),
            details: format!("Failed to spawn command: {}", e),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        let summary = if code == 0 {
            format!("Command completed successfully (exit code 0)")
        } else {
            format!("Command finished with exit code {}", code)
        };

        Ok(ToolResult {
            summary,
            output: json!({
                "success": code == 0,
                "exit_code": code,
                "stdout": stdout,
                "stderr": stderr
            }),
            success: code == 0,
        })
    }
}

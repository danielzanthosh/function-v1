use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::process::Command;

pub struct TerminalTool;

impl TerminalTool {
    pub fn new() -> Self {
        Self
    }

    fn is_destructive_command(cmd: &str) -> bool {
        let lower = cmd.to_lowercase();
        lower.contains("rmdir")
            || lower.contains("rm -rf")
            || lower.contains("format ")
            || lower.contains("del /f")
            || lower.contains("drop database")
    }
}

impl Default for TerminalTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TerminalTool {
    fn name(&self) -> &str {
        "computer_terminal"
    }

    fn description(&self) -> &str {
        "Execute a safe terminal or PowerShell command and inspect standard output."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The exact shell command line string to execute" },
                "cwd": { "type": "string", "description": "Optional working directory" }
            },
            "required": ["command"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let command = params.get("command").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'command' parameter".into(),
            }
        })?;

        if Self::is_destructive_command(command) && !ctx.allow_sensitive {
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
            format!("Command failed with exit code {}", code)
        };

        Ok(ToolResult {
            summary,
            output: json!({
                "exit_code": code,
                "stdout": stdout,
                "stderr": stderr
            }),
            success: code == 0,
        })
    }
}

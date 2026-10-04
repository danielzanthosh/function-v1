use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::fs;
use std::path::Path;

pub struct FileTool;

impl FileTool {
    pub fn new() -> Self {
        Self
    }
    fn is_system_path(path_str: &str) -> bool {
        let lower = path_str.to_lowercase();
        lower.contains(r"windows\system32")
            || lower.contains("/system32")
            || lower.contains("/etc/")
            || lower.contains("/bin/")
            || lower.contains("/usr/bin")
            || lower.contains(r"windows\regedit")
    }
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "computer_files"
    }

    fn description(&self) -> &str {
        "Inspect, read, write, list, move, or delete local files and directories."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "list", "exists", "delete", "move"],
                    "description": "File operation to perform"
                },
                "path": { "type": "string", "description": "Absolute or relative file path" },
                "content": { "type": "string", "description": "Content to write when action is 'write'" },
                "destination": { "type": "string", "description": "Target destination path when action is 'move'" }
            },
            "required": ["action", "path"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    fn permission_level(&self, params: &serde_json::Value) -> crate::ToolPermissionLevel {
        let path_str = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if Self::is_system_path(path_str) {
            return crate::ToolPermissionLevel::Restricted;
        }

        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if action == "delete" || action == "move" {
            crate::ToolPermissionLevel::Confirm
        } else {
            crate::ToolPermissionLevel::Safe
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'action' parameter".into(),
            })?;

        let path_str = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'path' parameter".into(),
            }
        })?;

        if Self::is_system_path(path_str) {
            return Err(ToolError::ExecutionFailed {
                tool: self.name().into(),
                details: format!(
                    "Access to critical system path '{}' is restricted.",
                    path_str
                ),
            });
        }

        let target_path = Path::new(path_str);

        match action {
            "read" => {
                let content =
                    fs::read_to_string(target_path).map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: format!("Failed to read file '{}': {}", path_str, e),
                    })?;
                let char_len = content.chars().count();
                Ok(ToolResult::success(
                    format!("Read {} characters from '{}'", char_len, path_str),
                    json!({ "path": path_str, "content": content, "length": char_len }),
                ))
            }
            "write" => {
                let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(parent) = target_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(target_path, content).map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: format!("Failed to write to '{}': {}", path_str, e),
                })?;
                Ok(ToolResult::success(
                    format!("Wrote {} bytes to '{}'", content.len(), path_str),
                    json!({ "path": path_str, "bytes_written": content.len() }),
                ))
            }
            "list" => {
                let entries =
                    fs::read_dir(target_path).map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: format!("Failed to list directory '{}': {}", path_str, e),
                    })?;
                let mut names = Vec::new();
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    names.push(json!({ "name": file_name, "is_directory": is_dir }));
                }
                Ok(ToolResult::success(
                    format!("Found {} entries in '{}'", names.len(), path_str),
                    json!({ "path": path_str, "entries": names }),
                ))
            }
            "exists" => {
                let exists = target_path.exists();
                Ok(ToolResult::success(
                    format!("Path '{}' exists: {}", path_str, exists),
                    json!({ "path": path_str, "exists": exists }),
                ))
            }
            "delete" => {
                if !ctx.allow_sensitive {
                    return Err(ToolError::RequiresConfirmation);
                }
                if target_path.is_dir() {
                    fs::remove_dir_all(target_path).map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: format!("Failed to delete directory '{}': {}", path_str, e),
                    })?;
                } else {
                    fs::remove_file(target_path).map_err(|e| ToolError::ExecutionFailed {
                        tool: self.name().into(),
                        details: format!("Failed to delete file '{}': {}", path_str, e),
                    })?;
                }
                Ok(ToolResult::success(
                    format!("Deleted '{}'", path_str),
                    json!({ "path": path_str, "status": "deleted" }),
                ))
            }
            "move" => {
                if !ctx.allow_sensitive {
                    return Err(ToolError::RequiresConfirmation);
                }
                let dest_str = params
                    .get("destination")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Missing 'destination' parameter for action 'move'".into(),
                    })?;
                let dest_path = Path::new(dest_str);
                fs::rename(target_path, dest_path).map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: format!("Failed to move '{}' to '{}': {}", path_str, dest_str, e),
                })?;
                Ok(ToolResult::success(
                    format!("Moved '{}' to '{}'", path_str, dest_str),
                    json!({ "source": path_str, "destination": dest_str }),
                ))
            }
            other => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown file action: '{}'", other),
            }),
        }
    }
}

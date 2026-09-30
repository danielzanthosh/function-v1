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
        "Inspect, read, write, or list local files and directories."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "list", "exists"]
                },
                "path": { "type": "string", "description": "Absolute or relative file path" },
                "content": { "type": "string", "description": "Content to write when action is 'write'" }
            },
            "required": ["action", "path"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        // By default, writing files might require confirmation if not in temp/workspace
        false
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = params.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'action' parameter".into(),
            }
        })?;

        let path_str = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Missing 'path' parameter".into(),
            }
        })?;

        let target_path = Path::new(path_str);

        match action {
            "read" => {
                let content = fs::read_to_string(target_path).map_err(|e| ToolError::ExecutionFailed {
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
                if !ctx.allow_sensitive && target_path.exists() {
                    // Check if overwrite might be sensitive
                }
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
                let entries = fs::read_dir(target_path).map_err(|e| ToolError::ExecutionFailed {
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
            other => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: format!("Unknown file action: '{}'", other),
            }),
        }
    }
}

//! Controlled computer capabilities exposed to the AI agent.
//!
//! Tools provide sandboxed, transparent, and user-confirmable access to the computer.
//! The LLM never executes arbitrary OS operations directly; all actions pass through this layer.

pub mod builtin;
pub use builtin::*;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool '{0}' not found")]
    NotFound(String),
    #[error("Invalid parameters for tool '{tool}': {details}")]
    InvalidParameters { tool: String, details: String },
    #[error("Execution error in tool '{tool}': {details}")]
    ExecutionFailed { tool: String, details: String },
    #[error("Operation requires user confirmation")]
    RequiresConfirmation,
    #[error("Action canceled by user")]
    Canceled,
}

/// Result returned after executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Concise description of the completed action for user feedback (e.g., "Opened Chrome").
    pub summary: String,
    /// Detailed structured or text output returned to the LLM agent.
    pub output: serde_json::Value,
    /// True if the tool execution succeeded.
    pub success: bool,
}

impl ToolResult {
    pub fn success(summary: impl Into<String>, output: serde_json::Value) -> Self {
        Self {
            summary: summary.into(),
            output,
            success: true,
        }
    }

    pub fn failure(summary: impl Into<String>, error_details: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            output: serde_json::json!({ "error": error_details.into() }),
            success: false,
        }
    }
}

/// Execution context provided to tools during execution.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub session_id: String,
    pub allow_sensitive: bool,
}

/// Tool permission category per docs/10-TOOLS.md and docs/16-SECURITY.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToolPermissionLevel {
    /// Safe to execute without user intervention.
    #[default]
    Safe,
    /// Potentially sensitive action requiring explicit user confirmation.
    Confirm,
    /// Restricted action completely prohibited from automated execution.
    Restricted,
}

/// Core trait implemented by all controlled computer tools.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique identifier for the tool (e.g., "browser_navigate").
    fn name(&self) -> &str;

    /// Human-readable explanation of what this tool does.
    fn description(&self) -> &str;

    /// JSON schema describing the expected parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Whether this tool modifies critical state and requires explicit user confirmation.
    fn requires_confirmation(&self) -> bool {
        false
    }

    /// Return the permission level for this specific invocation.
    fn permission_level(&self, _params: &serde_json::Value) -> ToolPermissionLevel {
        if self.requires_confirmation() {
            ToolPermissionLevel::Confirm
        } else {
            ToolPermissionLevel::Safe
        }
    }

    /// Execute the tool action with the supplied parameters.
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult, ToolError>;
}

/// Registry of available tools for discovery and dispatching.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a new tool into the registry.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    /// Find a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tools.
    pub fn list(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// Execute a tool by name with parameters and permission enforcement.
    pub async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool = self.get(name).ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        let level = tool.permission_level(&params);
        if level == ToolPermissionLevel::Restricted {
            return Err(ToolError::ExecutionFailed {
                tool: name.to_string(),
                details: "Action is classified as RESTRICTED and prohibited from automated execution.".into(),
            });
        }
        if (level == ToolPermissionLevel::Confirm || tool.requires_confirmation()) && !ctx.allow_sensitive {
            return Err(ToolError::RequiresConfirmation);
        }
        tool.execute(params, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes input text"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                },
                "required": ["text"]
            })
        }

        async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
            let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolResult::success(format!("Echoed: {}", text), serde_json::json!({ "result": text })))
        }
    }

    #[tokio::test]
    async fn test_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        assert!(registry.get("echo").is_some());
        let res = registry
            .execute("echo", serde_json::json!({ "text": "hello" }), &ToolContext::default())
            .await
            .unwrap();
        assert_eq!(res.summary, "Echoed: hello");
    }

    #[tokio::test]
    async fn test_default_tools_registration() {
        let mut registry = ToolRegistry::new();
        register_default_tools(&mut registry);
        assert!(registry.get("computer_mouse").is_some());
        assert!(registry.get("computer_keyboard").is_some());
        assert!(registry.get("computer_screen").is_some());
        assert!(registry.get("computer_apps").is_some());
        assert!(registry.get("browser_control").is_some());
        assert!(registry.get("web_search").is_some());
        assert!(registry.get("computer_files").is_some());
        assert!(registry.get("computer_terminal").is_some());

        // Test screen dimension execution
        let res = registry
            .execute("computer_screen", serde_json::json!({ "action": "dimensions" }), &ToolContext::default())
            .await
            .unwrap();
        assert!(res.success);
    }

    #[tokio::test]
    async fn test_security_permission_levels() {
        let mut registry = ToolRegistry::new();
        register_default_tools(&mut registry);

        // 1. Restricted command must be blocked immediately
        let restricted_res = registry
            .execute(
                "computer_terminal",
                serde_json::json!({ "command": "format c: /fs:ntfs" }),
                &ToolContext { session_id: "s1".into(), allow_sensitive: true },
            )
            .await;
        assert!(restricted_res.is_err());
        assert!(matches!(restricted_res.unwrap_err(), ToolError::ExecutionFailed { .. }));

        // 2. Sensitive command requires confirmation
        let confirm_res = registry
            .execute(
                "computer_terminal",
                serde_json::json!({ "command": "dir" }),
                &ToolContext { session_id: "s1".into(), allow_sensitive: false },
            )
            .await;
        assert!(confirm_res.is_err());
        assert!(matches!(confirm_res.unwrap_err(), ToolError::RequiresConfirmation));

        // 3. File delete requires confirmation
        let del_res = registry
            .execute(
                "computer_files",
                serde_json::json!({ "action": "delete", "path": "test.tmp" }),
                &ToolContext { session_id: "s1".into(), allow_sensitive: false },
            )
            .await;
        assert!(del_res.is_err());
        assert!(matches!(del_res.unwrap_err(), ToolError::RequiresConfirmation));
    }
}

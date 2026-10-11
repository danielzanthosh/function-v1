use crate::{Tool, ToolContext, ToolError, ToolPermissionLevel, ToolResult};
use async_trait::async_trait;
use function_memory::{MemoryCategory, MemoryItem, MemoryStore};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Gives the agent explicit, user-directed controls for persistent memory.
/// Memory is only written when the model calls this tool; it is not inferred
/// from every conversation by silently saving the whole transcript.
pub struct MemoryTool {
    store: Arc<dyn MemoryStore>,
}

impl MemoryTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }

    fn category(value: &str) -> Option<MemoryCategory> {
        match value {
            "user_preference" => Some(MemoryCategory::UserPreference),
            "frequent_application" => Some(MemoryCategory::FrequentApplication),
            "frequent_website" => Some(MemoryCategory::FrequentWebsite),
            "task_summary" => Some(MemoryCategory::TaskSummary),
            "custom_instruction" => Some(MemoryCategory::CustomInstruction),
            _ => None,
        }
    }

    fn category_name(category: MemoryCategory) -> &'static str {
        match category {
            MemoryCategory::UserPreference => "user_preference",
            MemoryCategory::FrequentApplication => "frequent_application",
            MemoryCategory::FrequentWebsite => "frequent_website",
            MemoryCategory::TaskSummary => "task_summary",
            MemoryCategory::CustomInstruction => "custom_instruction",
        }
    }

    fn contains_secret_hint(text: &str) -> bool {
        let lower = text.to_lowercase();
        [
            "password=", "api_key=", "api key:", "access token:", "refresh token:",
            "private key", "recovery phrase", "recovery code", "secret_key=",
        ]
        .iter()
        .any(|pattern| lower.contains(pattern))
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[async_trait]
impl Tool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "Manage Function's small persistent memory across app launches. Use action='remember' only when the user explicitly asks you to remember/save a stable preference or useful fact; never save passwords, API keys, tokens, private keys, or other secrets. Use action='recall' when the user asks what you remember or when a saved fact is relevant. Use action='forget' to delete one item by its returned id, or action='clear_category' only when the user explicitly asks to clear that category. This stores concise notes, not full conversation transcripts."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["remember", "recall", "forget", "clear_category"],
                    "description": "Operation to perform."
                },
                "category": {
                    "type": "string",
                    "enum": ["user_preference", "frequent_application", "frequent_website", "task_summary", "custom_instruction"],
                    "description": "Memory category. Required for remember and clear_category."
                },
                "key": { "type": "string", "description": "Short descriptive label for a memory." },
                "value": { "type": "string", "description": "Concise fact to remember." },
                "query": { "type": "string", "description": "Words to search for in saved memories; leave blank to list recent items." },
                "id": { "type": "string", "description": "Exact memory id to delete." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20, "description": "Maximum number of recalled items." }
            },
            "required": ["action"],
            "additionalProperties": false
        })
    }

    fn permission_level(&self, _params: &Value) -> ToolPermissionLevel {
        ToolPermissionLevel::Safe
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = params.get("action").and_then(Value::as_str).unwrap_or("");
        match action {
            "remember" => {
                let category_name = params.get("category").and_then(Value::as_str).unwrap_or("");
                let category = Self::category(category_name).ok_or_else(|| ToolError::InvalidParameters {
                    tool: self.name().into(),
                    details: "A valid category is required.".into(),
                })?;
                let key = params.get("key").and_then(Value::as_str).unwrap_or("").trim();
                let value = params.get("value").and_then(Value::as_str).unwrap_or("").trim();
                if key.is_empty() || key.chars().count() > 120 || value.is_empty() || value.chars().count() > 1000 {
                    return Err(ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Provide a non-empty key (up to 120 characters) and value (up to 1000 characters).".into(),
                    });
                }
                if Self::contains_secret_hint(key) || Self::contains_secret_hint(value) {
                    return Ok(ToolResult::failure(
                        "Memory not saved",
                        "This looks like a credential or secret. Function will not store it in persistent memory.",
                    ));
                }
                // A stable ID means remembering the same category/key updates the existing note.
                let safe_key: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
                let item = MemoryItem {
                    id: format!("{}:{}", category_name, safe_key.trim_matches('-')),
                    category,
                    key: key.to_string(),
                    value: value.to_string(),
                    score: 1.0,
                    created_at: Self::now(),
                };
                self.store.remember(item).await.map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                Ok(ToolResult::success(
                    format!("Saved memory: {}", key),
                    json!({ "saved": true, "key": key, "category": category_name }),
                ))
            }
            "recall" => {
                let query = params.get("query").and_then(Value::as_str).unwrap_or("");
                let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(5).clamp(1, 20) as usize;
                let category = match params.get("category").and_then(Value::as_str) {
                    Some(name) => Some(Self::category(name).ok_or_else(|| ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Invalid category.".into(),
                    })?),
                    None => None,
                };
                let items = self.store.recall(query, category, limit).await.map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                let output: Vec<Value> = items.iter().map(|item| json!({
                    "id": item.id,
                    "category": Self::category_name(item.category),
                    "key": item.key,
                    "value": item.value,
                    "created_at": item.created_at
                })).collect();
                Ok(ToolResult::success(
                    format!("Found {} saved memories", output.len()),
                    json!({ "memories": output }),
                ))
            }
            "forget" => {
                let id = params.get("id").and_then(Value::as_str).unwrap_or("").trim();
                if id.is_empty() {
                    return Err(ToolError::InvalidParameters {
                        tool: self.name().into(),
                        details: "Provide the exact memory id to delete.".into(),
                    });
                }
                self.store.forget(id).await.map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                Ok(ToolResult::success("Memory deleted", json!({ "deleted": true, "id": id })))
            }
            "clear_category" => {
                let category_name = params.get("category").and_then(Value::as_str).unwrap_or("");
                let category = Self::category(category_name).ok_or_else(|| ToolError::InvalidParameters {
                    tool: self.name().into(),
                    details: "A valid category is required.".into(),
                })?;
                self.store.clear_category(category).await.map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().into(),
                    details: e.to_string(),
                })?;
                Ok(ToolResult::success(
                    format!("Cleared {} memories", category_name),
                    json!({ "cleared": true, "category": category_name }),
                ))
            }
            _ => Err(ToolError::InvalidParameters {
                tool: self.name().into(),
                details: "Choose remember, recall, forget, or clear_category.".into(),
            }),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use function_memory::InMemoryMemoryStore;

    #[tokio::test]
    async fn memory_tool_remembers_and_recalls_a_note() {
        let store = Arc::new(InMemoryMemoryStore::new());
        let tool = MemoryTool::new(store);
        let ctx = ToolContext::default();

        let saved = tool.execute(json!({
            "action": "remember",
            "category": "user_preference",
            "key": "response_style",
            "value": "Keep answers concise"
        }), &ctx).await.unwrap();
        assert!(saved.success);

        let recalled = tool.execute(json!({
            "action": "recall",
            "query": "concise answers",
            "limit": 5
        }), &ctx).await.unwrap();
        assert!(recalled.success);
        assert_eq!(recalled.output["memories"][0]["value"], "Keep answers concise");
    }

    #[tokio::test]
    async fn memory_tool_refuses_secret_like_values() {
        let store = Arc::new(InMemoryMemoryStore::new());
        let tool = MemoryTool::new(store.clone());
        let result = tool.execute(json!({
            "action": "remember",
            "category": "custom_instruction",
            "key": "api_key",
            "value": "api_key=do-not-store"
        }), &ToolContext::default()).await.unwrap();
        assert!(!result.success);

        let memories = store.recall("", None, 10).await.unwrap();
        assert!(memories.is_empty());
    }

    #[tokio::test]
    async fn memory_tool_can_forget_a_saved_note() {
        let store = Arc::new(InMemoryMemoryStore::new());
        let tool = MemoryTool::new(store);
        let ctx = ToolContext::default();
        tool.execute(json!({
            "action": "remember",
            "category": "user_preference",
            "key": "theme",
            "value": "dark"
        }), &ctx).await.unwrap();

        tool.execute(json!({
            "action": "forget",
            "id": "user_preference:theme"
        }), &ctx).await.unwrap();
        let memories = tool.execute(json!({"action": "recall"}), &ctx).await.unwrap();
        assert_eq!(memories.output["memories"].as_array().unwrap().len(), 0);
    }
}

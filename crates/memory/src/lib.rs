//! Persistent memory and context retrieval.
//!
//! Stores user preferences, frequently used apps/sites, and task history.
//! Context is retrieved selectively based on relevance rather than injected globally.

pub mod file_store;
pub use file_store::FileMemoryStore;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Storage I/O failure: {0}")]
    Io(String),
    #[error("Serialization failure: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Database error: {0}")]
    Database(String),
}

/// Category of contextual memory item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    UserPreference,
    FrequentApplication,
    FrequentWebsite,
    TaskSummary,
    CustomInstruction,
}

/// A stored memory item with metadata and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub category: MemoryCategory,
    pub key: String,
    pub value: String,
    pub score: f32,
    pub created_at: u64,
}

/// Storage interface for assistant context and memory.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Save or update a memory item.
    async fn remember(&self, item: MemoryItem) -> Result<(), MemoryError>;

    /// Retrieve items relevant to a query text or category.
    async fn recall(
        &self,
        query: &str,
        category: Option<MemoryCategory>,
        limit: usize,
    ) -> Result<Vec<MemoryItem>, MemoryError>;

    /// Delete a memory item by ID.
    async fn forget(&self, id: &str) -> Result<(), MemoryError>;

    /// Clear all memory items in a category.
    async fn clear_category(&self, category: MemoryCategory) -> Result<(), MemoryError>;
}

/// In-memory implementation of MemoryStore for testing and default runtime fallback.
#[derive(Default)]
pub struct InMemoryMemoryStore {
    items: RwLock<HashMap<String, MemoryItem>>,
}

impl InMemoryMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MemoryStore for InMemoryMemoryStore {
    async fn remember(&self, item: MemoryItem) -> Result<(), MemoryError> {
        let mut write = self
            .items
            .write()
            .map_err(|e| MemoryError::Database(e.to_string()))?;
        write.insert(item.id.clone(), item);
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        category: Option<MemoryCategory>,
        limit: usize,
    ) -> Result<Vec<MemoryItem>, MemoryError> {
        let read = self
            .items
            .read()
            .map_err(|e| MemoryError::Database(e.to_string()))?;
        let query_lower = query.to_lowercase();

        // Simple greetings or empty queries should not trigger arbitrary application launches or unrelated task summaries
        let is_casual_convo = query_lower.len() <= 12
            && (query_lower.contains("hi")
                || query_lower.contains("hello")
                || query_lower.contains("hey")
                || query_lower.contains("greeting")
                || query_lower.contains("what's up")
                || query_lower.contains("sup")
                || query_lower.contains("good morning")
                || query_lower.contains("good evening")
                || query_lower.contains("howdy"));

        let mut matches: Vec<MemoryItem> = read
            .values()
            .filter(|item| {
                if is_casual_convo && item.category == MemoryCategory::TaskSummary {
                    return false;
                }
                if let Some(cat) = category {
                    if item.category != cat {
                        return false;
                    }
                }
                if query_lower.is_empty() || is_casual_convo {
                    return item.category == MemoryCategory::UserPreference
                        || item.category == MemoryCategory::CustomInstruction;
                }
                item.key.to_lowercase().contains(&query_lower)
                    || item.value.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        matches.truncate(limit);
        Ok(matches)
    }

    async fn forget(&self, id: &str) -> Result<(), MemoryError> {
        let mut write = self
            .items
            .write()
            .map_err(|e| MemoryError::Database(e.to_string()))?;
        write.remove(id);
        Ok(())
    }

    async fn clear_category(&self, category: MemoryCategory) -> Result<(), MemoryError> {
        let mut write = self
            .items
            .write()
            .map_err(|e| MemoryError::Database(e.to_string()))?;
        write.retain(|_, v| v.category != category);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_memory_store() {
        let store = InMemoryMemoryStore::new();
        let item = MemoryItem {
            id: "pref-1".into(),
            category: MemoryCategory::UserPreference,
            key: "default_browser".into(),
            value: "Chrome".into(),
            score: 1.0,
            created_at: 0,
        };
        store.remember(item).await.unwrap();

        let recalled = store.recall("browser", None, 5).await.unwrap();
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].value, "Chrome");

        store.forget("pref-1").await.unwrap();
        let recalled_empty = store.recall("browser", None, 5).await.unwrap();
        assert!(recalled_empty.is_empty());
    }

    #[tokio::test]
    async fn test_file_memory_store() {
        let temp_dir = std::env::temp_dir().join(format!(
            "function_test_{}",
            std::time::SystemTime::now().elapsed().unwrap().as_nanos()
        ));
        let file_path = temp_dir.join("memory.json");

        let store = FileMemoryStore::new(&file_path).unwrap();
        let item = MemoryItem {
            id: "task-1".into(),
            category: MemoryCategory::TaskSummary,
            key: "last_search".into(),
            value: "Rust async tutorial".into(),
            score: 0.9,
            created_at: 100,
        };
        store.remember(item).await.unwrap();

        // Verify disk existence
        assert!(file_path.exists());

        // Re-open from disk
        let store2 = FileMemoryStore::new(&file_path).unwrap();
        let results = store2
            .recall("Rust", Some(MemoryCategory::TaskSummary), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "last_search");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

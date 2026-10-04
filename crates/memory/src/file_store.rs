use crate::{MemoryCategory, MemoryError, MemoryItem, MemoryStore};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Persistent file-backed implementation of MemoryStore.
pub struct FileMemoryStore {
    file_path: PathBuf,
    cache: RwLock<HashMap<String, MemoryItem>>,
}

impl FileMemoryStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, MemoryError> {
        let file_path = path.as_ref().to_path_buf();
        let mut items = HashMap::new();

        if file_path.exists() {
            let data = fs::read_to_string(&file_path)
                .map_err(|e| MemoryError::Io(format!("Failed to read memory file: {}", e)))?;
            if !data.trim().is_empty() {
                let list: Vec<MemoryItem> = serde_json::from_str(&data)?;
                for item in list {
                    items.insert(item.id.clone(), item);
                }
            }
        }

        Ok(Self {
            file_path,
            cache: RwLock::new(items),
        })
    }

    fn persist(&self) -> Result<(), MemoryError> {
        let read = self
            .cache
            .read()
            .map_err(|e| MemoryError::Database(e.to_string()))?;
        let items: Vec<MemoryItem> = read.values().cloned().collect();
        let serialized = serde_json::to_string_pretty(&items)?;

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                MemoryError::Io(format!("Failed to create memory directory: {}", e))
            })?;
        }

        let tmp_path = self.file_path.with_extension("tmp");
        fs::write(&tmp_path, serialized).map_err(|e| {
            MemoryError::Io(format!("Failed to write temporary memory file: {}", e))
        })?;
        fs::rename(&tmp_path, &self.file_path)
            .map_err(|e| MemoryError::Io(format!("Failed to persist memory file: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl MemoryStore for FileMemoryStore {
    async fn remember(&self, item: MemoryItem) -> Result<(), MemoryError> {
        {
            let mut write = self
                .cache
                .write()
                .map_err(|e| MemoryError::Database(e.to_string()))?;
            write.insert(item.id.clone(), item);
        }
        self.persist()
    }

    async fn recall(
        &self,
        query: &str,
        category: Option<MemoryCategory>,
        limit: usize,
    ) -> Result<Vec<MemoryItem>, MemoryError> {
        let read = self
            .cache
            .read()
            .map_err(|e| MemoryError::Database(e.to_string()))?;
        let query_lower = query.to_lowercase();

        // Simple greetings or general queries should not recall arbitrary previous task summaries
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

        // Sort by relevance score descending
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Cap recalled context items and summarize long items to ensure context remains under max 250 lines
        matches.truncate(limit);
        for item in &mut matches {
            let lines: Vec<&str> = item.value.lines().collect();
            if lines.len() > 250 {
                let summarized = lines[..250].join("\n");
                item.value = format!("{}\n...[summarized to max 250 lines]", summarized);
            }
        }

        Ok(matches)
    }

    async fn forget(&self, id: &str) -> Result<(), MemoryError> {
        {
            let mut write = self
                .cache
                .write()
                .map_err(|e| MemoryError::Database(e.to_string()))?;
            write.remove(id);
        }
        self.persist()
    }

    async fn clear_category(&self, category: MemoryCategory) -> Result<(), MemoryError> {
        {
            let mut write = self
                .cache
                .write()
                .map_err(|e| MemoryError::Database(e.to_string()))?;
            write.retain(|_, v| v.category != category);
        }
        self.persist()
    }
}

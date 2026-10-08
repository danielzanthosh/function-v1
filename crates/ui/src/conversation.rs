//! Persistent conversation and chat history management for Function.
//!
//! Stores previous conversations in `~/.function/conversations.json` so users
//! can browse old chats, restore full conversational context, and start new chats.

use function_config::conversations_path;
use function_providers::ChatMessage;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single message turn in the visible chat log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatEntry {
    pub is_user: bool,
    pub text: String,
}

/// A stored multi-turn conversation with metadata and full message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConversation {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub display_messages: Vec<ChatEntry>,
    pub api_messages: Vec<ChatMessage>,
}

impl SavedConversation {
    pub fn new(display_messages: Vec<ChatEntry>, api_messages: Vec<ChatMessage>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let id = format!("conv_{}", now);
        let title = Self::derive_title(&display_messages);

        Self {
            id,
            title,
            created_at: now,
            updated_at: now,
            display_messages,
            api_messages,
        }
    }

    /// Derive an intuitive title from the first user prompt.
    pub fn derive_title(messages: &[ChatEntry]) -> String {
        for msg in messages {
            if msg.is_user {
                let clean = msg.text.trim();
                if !clean.is_empty() {
                    let first_line = clean.lines().next().unwrap_or(clean);
                    if first_line.chars().count() > 42 {
                        let mut title: String = first_line.chars().take(42).collect();
                        title.push_str("…");
                        return title;
                    }
                    return first_line.to_string();
                }
            }
        }
        "New Conversation".to_string()
    }

    /// Formatted preview of the last assistant reply or message.
    pub fn preview(&self) -> String {
        if let Some(last) = self.display_messages.last() {
            let text = last.text.trim();
            if text.chars().count() > 60 {
                let mut truncated: String = text.chars().take(60).collect();
                truncated.push_str("…");
                truncated
            } else {
                text.to_string()
            }
        } else {
            "Empty conversation".to_string()
        }
    }
}

/// Persistent store managing user conversations in `~/.function/conversations.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationStore {
    pub conversations: Vec<SavedConversation>,
}

impl ConversationStore {
    /// Load conversations from disk.
    pub fn load() -> Self {
        let path = conversations_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(store) = serde_json::from_str::<Self>(&content) {
                    return store;
                }
            }
        }
        Self::default()
    }

    /// Persist conversations to disk atomically.
    pub fn save(&self) -> Result<(), String> {
        let path = conversations_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize conversations: {}", e))?;

        let tmp = path.with_extension("tmp");
        fs::write(&tmp, json)
            .map_err(|e| format!("Failed to write temporary conversations file: {}", e))?;
        fs::rename(&tmp, &path)
            .map_err(|e| format!("Failed to persist conversations file: {}", e))?;

        Ok(())
    }

    /// Return all saved conversations sorted by most recent first.
    pub fn list(&self) -> Vec<SavedConversation> {
        let mut list = self.conversations.clone();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    /// Retrieve a conversation by ID.
    pub fn get(&self, id: &str) -> Option<&SavedConversation> {
        self.conversations.iter().find(|c| c.id == id)
    }

    /// Save or update a conversation.
    pub fn save_conversation(&mut self, mut conv: SavedConversation) {
        conv.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Some(idx) = self.conversations.iter().position(|c| c.id == conv.id) {
            self.conversations[idx] = conv;
        } else {
            self.conversations.push(conv);
        }
        let _ = self.save();
    }

    /// Delete a conversation by ID.
    pub fn delete_conversation(&mut self, id: &str) -> bool {
        let initial_len = self.conversations.len();
        self.conversations.retain(|c| c.id != id);
        let removed = self.conversations.len() < initial_len;
        if removed {
            let _ = self.save();
        }
        removed
    }

    /// Clear all conversation history.
    pub fn clear_all(&mut self) {
        self.conversations.clear();
        let _ = self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saved_conversation_derives_title() {
        let messages = vec![
            ChatEntry {
                is_user: true,
                text: "What is the capital of France?".to_string(),
            },
            ChatEntry {
                is_user: false,
                text: "Paris".to_string(),
            },
        ];

        let conv = SavedConversation::new(messages, Vec::new());
        assert_eq!(conv.title, "What is the capital of France?");
        assert_eq!(conv.display_messages.len(), 2);
    }

    #[test]
    fn test_saved_conversation_unicode_safety() {
        let emoji_msg = vec![ChatEntry {
            is_user: true,
            text: "🚀 Highly complex prompt with multi-byte unicode 🎉 and emojis that exceed forty-two characters long!".to_string(),
        }];
        let conv = SavedConversation::new(emoji_msg.clone(), Vec::new());
        assert!(conv.title.ends_with('…'));
        assert_eq!(conv.title.chars().count(), 43); // 42 chars + 1 ellipsis

        let preview_msg = SavedConversation::new(
            vec![ChatEntry {
                is_user: false,
                text: "🌟 Highly complex response with multi-byte unicode 💫 and accented characters like café, résumé, and ñ! Extra long text to trigger preview truncation.".to_string(),
            }],
            Vec::new(),
        );
        let preview = preview_msg.preview();
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), 61); // 60 chars + 1 ellipsis
    }

    #[test]
    fn test_conversation_store_crud() {
        let mut store = ConversationStore::default();
        let conv = SavedConversation::new(
            vec![ChatEntry {
                is_user: true,
                text: "Test query".to_string(),
            }],
            Vec::new(),
        );

        let id = conv.id.clone();
        store.save_conversation(conv);
        assert_eq!(store.list().len(), 1);
        assert!(store.get(&id).is_some());

        assert!(store.delete_conversation(&id));
        assert_eq!(store.list().len(), 0);
    }
}

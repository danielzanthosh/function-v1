//! Configuration and secure credential abstractions.
//!
//! Maintains settings for AI providers, platform behavior, hotkeys, and permissions.
//! Secrets are strictly accessed via secure credential abstractions and never stored in plaintext.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Credential error: {0}")]
    Credential(#[from] CredentialError),
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

#[derive(Error, Debug)]
pub enum CredentialError {
    #[error("Credential not found for key: {0}")]
    NotFound(String),
    #[error("Storage backend error: {0}")]
    BackendError(String),
}

/// Secure credential storage abstraction.
pub trait CredentialStore: Send + Sync {
    fn get_secret(&self, key: &str) -> Result<Option<String>, CredentialError>;
    fn set_secret(&self, key: &str, value: &str) -> Result<(), CredentialError>;
    fn delete_secret(&self, key: &str) -> Result<(), CredentialError>;
}

/// In-memory credential store suitable for testing and local development fallbacks.
#[derive(Default)]
pub struct InMemoryCredentialStore {
    secrets: RwLock<HashMap<String, String>>,
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn get_secret(&self, key: &str) -> Result<Option<String>, CredentialError> {
        let read = self
            .secrets
            .read()
            .map_err(|e| CredentialError::BackendError(e.to_string()))?;
        Ok(read.get(key).cloned())
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<(), CredentialError> {
        let mut write = self
            .secrets
            .write()
            .map_err(|e| CredentialError::BackendError(e.to_string()))?;
        write.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete_secret(&self, key: &str) -> Result<(), CredentialError> {
        let mut write = self
            .secrets
            .write()
            .map_err(|e| CredentialError::BackendError(e.to_string()))?;
        write.remove(key);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeStyle {
    #[default]
    CarbonDark,
    ObsidianOled,
    SlateMidnight,
    StudioLight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccentColor {
    #[default]
    White,
    Cyan,
    Emerald,
    Violet,
    Amber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WindowPositionMode {
    #[default]
    Center,
    UpperThird,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub hotkey: String,
    pub theme: ThemePreference,
    #[serde(default)]
    pub theme_style: ThemeStyle,
    #[serde(default)]
    pub accent_color: AccentColor,
    #[serde(default)]
    pub window_position: WindowPositionMode,
    #[serde(default = "default_sound_enabled")]
    pub sound_enabled: bool,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    pub ai_provider: AiProviderConfig,
    pub speech: SpeechConfig,
    pub search: SearchConfig,
    pub memory: MemoryConfig,
}

fn default_sound_enabled() -> bool {
    true
}

fn default_start_hidden() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: if cfg!(target_os = "macos") {
                "Command+;".to_string()
            } else {
                "Ctrl+Space".to_string()
            },
            theme: ThemePreference::System,
            theme_style: ThemeStyle::CarbonDark,
            accent_color: AccentColor::White,
            window_position: WindowPositionMode::UpperThird,
            sound_enabled: true,
            start_hidden: true,
            ai_provider: AiProviderConfig::default(),
            speech: SpeechConfig::default(),
            search: SearchConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl AppConfig {
    /// Return standard base directory `~/.function` across all platforms using a 3-tier cascade.
    pub fn function_dir() -> std::path::PathBuf {
        let base_dir = std::env::var("USERPROFILE")
            .map(std::path::PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(std::path::PathBuf::from))
            .unwrap_or_else(|_| std::env::temp_dir());
        base_dir.join(".function")
    }

    /// Return standard config file path in `~/.function/config.json`.
    pub fn config_path() -> std::path::PathBuf {
        Self::function_dir().join("config.json")
    }

    /// Return standard memory file path in `~/.function/memory.json`.
    pub fn memory_path() -> std::path::PathBuf {
        Self::function_dir().join("memory.json")
    }

    /// Load configuration from disk, falling back to default if file doesn't exist or is invalid.
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_json::from_str::<Self>(&content) {
                    // Preserve custom shortcuts while moving only the old shipped macOS
                    // default away from Option+Space.
                    if migrate_macos_default_hotkey(&mut cfg) {
                        let _ = cfg.save();
                    }
                    return cfg;
                }
            }
        }
        Self::default()
    }

    /// Save configuration to `~/.function/config.json`.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, serialized)?;
        Ok(())
    }
}

fn migrate_macos_default_hotkey(config: &mut AppConfig) -> bool {
    #[cfg(target_os = "macos")]
    {
        if config.hotkey == "Option+Space" {
            config.hotkey = "Command+;".to_string();
            return true;
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = config;

    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub provider_name: String,
    pub base_url: String,
    pub model: String,
    pub api_key_reference: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            provider_name: "openai-compatible".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            api_key_reference: "ai_api_key".to_string(),
            api_key: None,
        }
    }
}

impl AiProviderConfig {
    pub fn is_configured(&self) -> bool {
        if let Some(ref k) = self.api_key {
            if !k.trim().is_empty() {
                return true;
            }
        }
        std::env::var("OPENAI_API_KEY")
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false)
    }

    pub fn resolve_api_key(&self, credentials: &dyn CredentialStore) -> Option<String> {
        if let Some(ref k) = self.api_key {
            let trimmed = k.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.trim().is_empty() {
                return Some(key);
            }
        }
        credentials
            .get_secret(&self.api_key_reference)
            .ok()
            .flatten()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub auto_detect_language: bool,
    pub api_key_reference: Option<String>,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "whisper".to_string(),
            model: "whisper-1".to_string(),
            auto_detect_language: true,
            api_key_reference: None,
        }
    }
}

impl SpeechConfig {
    pub fn is_configured(&self) -> bool {
        self.enabled
            && self
                .api_key_reference
                .as_ref()
                .map_or(false, |k| !k.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub enabled: bool,
    pub provider: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "duckduckgo".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub database_path: Option<String>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            database_path: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_configuration() {
        let mut config = AppConfig::default();
        assert!(!config.ai_provider.is_configured());

        config.ai_provider.api_key = Some("sk-test-12345".to_string());
        assert!(config.ai_provider.is_configured());

        let store = InMemoryCredentialStore::new();
        assert_eq!(
            config.ai_provider.resolve_api_key(&store),
            Some("sk-test-12345".to_string())
        );
    }

    #[test]
    fn test_config_json_roundtrip() {
        let mut config = AppConfig::default();
        config.sound_enabled = false;
        config.ai_provider.api_key = Some("test-key".into());

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();

        assert!(!deserialized.sound_enabled);
        assert_eq!(
            deserialized.ai_provider.api_key.as_deref(),
            Some("test-key")
        );
    }

    #[test]
    fn test_legacy_macos_hotkey_migration_only_changes_old_default() {
        let mut config = AppConfig::default();
        config.hotkey = "Option+Space".to_string();

        let migrated = migrate_macos_default_hotkey(&mut config);

        #[cfg(target_os = "macos")]
        {
            assert!(migrated);
            assert_eq!(config.hotkey, "Command+;");
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert!(!migrated);
            assert_eq!(config.hotkey, "Option+Space");
        }
    }

    #[test]
    fn test_redact_secrets() {
        let input =
            "Calling OpenAI API with Authorization: Bearer sk-abcdef1234567890xyz and token test";
        let redacted = redact_secrets(input);
        assert!(!redacted.contains("sk-abcdef1234567890xyz"));
        assert!(redacted.contains("[REDACTED_API_KEY]"));
    }

    #[test]
    fn test_function_paths_resolution() {
        let dir = function_dir();
        assert!(dir.ends_with(".function"));
        let cfg_path = config_path();
        assert_eq!(cfg_path, dir.join("config.json"));
        let mem_path = memory_path();
        assert_eq!(mem_path, dir.join("memory.json"));
        assert_eq!(AppConfig::function_dir(), dir);
        assert_eq!(AppConfig::config_path(), cfg_path);
        assert_eq!(AppConfig::memory_path(), mem_path);
    }
}

/// Return standard base directory `~/.function` across all platforms using a 3-tier cascade.
pub fn function_dir() -> std::path::PathBuf {
    AppConfig::function_dir()
}

/// Return standard config file path in `~/.function/config.json`.
pub fn config_path() -> std::path::PathBuf {
    AppConfig::config_path()
}

/// Return standard memory file path in `~/.function/memory.json`.
pub fn memory_path() -> std::path::PathBuf {
    AppConfig::memory_path()
}

/// Redact sensitive API keys, authorization tokens, and credentials from text strings before logging.
pub fn redact_secrets(input: &str) -> String {
    let mut output = input.to_string();

    // Redact sk-... OpenAI keys
    let mut idx = 0;
    while let Some(start) = output[idx..].find("sk-") {
        let actual_start = idx + start;
        let mut end = actual_start + 3;
        while end < output.len() {
            let b = output.as_bytes()[end];
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                end += 1;
            } else {
                break;
            }
        }
        if end - actual_start >= 8 {
            let key = output[actual_start..end].to_string();
            output = output.replace(&key, "[REDACTED_API_KEY]");
            idx = actual_start + "[REDACTED_API_KEY]".len();
        } else {
            idx = end;
        }
        if idx >= output.len() {
            break;
        }
    }

    output
}

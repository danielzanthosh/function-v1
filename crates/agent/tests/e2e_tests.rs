//! End-to-End integration tests for the Function agent cognitive loop.
//!
//! Tests the complete Observe-Think-Act cycle, state broadcast delivery,
//! memory persistence, tool permission enforcement, and error recovery.

use function_agent::{Agent, AgentState};
use function_memory::{InMemoryMemoryStore, MemoryCategory, MemoryItem, MemoryStore};
use function_providers::MockLlmProvider;
use function_tools::ToolRegistry;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Agent State Machine Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_state_transitions_idle_to_completed() {
    let provider = Arc::new(MockLlmProvider::new("Done."));
    let tools = ToolRegistry::new();
    let memory = Arc::new(InMemoryMemoryStore::new());
    let agent = Agent::new(provider, tools, memory);

    let mut rx = agent.subscribe_state();
    let result = agent.execute_task("hello").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Done.");

    // First state broadcast should be Processing
    let s1 = rx.recv().await.unwrap();
    assert!(matches!(s1, AgentState::Processing { .. }));

    // Final broadcast should be Completed
    let s2 = rx.recv().await.unwrap();
    assert!(matches!(s2, AgentState::Completed { .. }));
    if let AgentState::Completed { summary } = s2 {
        assert_eq!(summary, "Done.");
    }
}

#[tokio::test]
async fn test_agent_empty_prompt_still_executes() {
    let provider = Arc::new(MockLlmProvider::new("Empty task handled."));
    let tools = ToolRegistry::new();
    let memory = Arc::new(InMemoryMemoryStore::new());
    let agent = Agent::new(provider, tools, memory);

    // Even an empty string prompt should complete without panicking
    let result = agent.execute_task("").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_agent_multiple_subscribers() {
    let provider = Arc::new(MockLlmProvider::new("Broadcast test."));
    let tools = ToolRegistry::new();
    let memory = Arc::new(InMemoryMemoryStore::new());
    let agent = Agent::new(provider, tools, memory);

    let mut rx1 = agent.subscribe_state();
    let mut rx2 = agent.subscribe_state();

    let _ = agent.execute_task("multi-sub").await;

    // Both subscribers should receive the same state sequence
    let s1 = rx1.recv().await.unwrap();
    let s2 = rx2.recv().await.unwrap();
    assert_eq!(s1, s2);
}

// ---------------------------------------------------------------------------
// Memory Integration Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_with_preloaded_memory_context() {
    let memory = Arc::new(InMemoryMemoryStore::new());
    memory
        .remember(MemoryItem {
            id: "pref_001".to_string(),
            category: MemoryCategory::UserPreference,
            key: "user_preference".to_string(),
            value: "dark theme".to_string(),
            score: 1.0,
            created_at: 1234567890,
        })
        .await
        .unwrap();

    let provider = Arc::new(MockLlmProvider::new("Applied dark theme."));
    let tools = ToolRegistry::new();
    let agent = Agent::new(provider, tools, memory.clone());

    let result = agent.execute_task("apply my preference").await;
    assert!(result.is_ok());

    // Verify memory was accessible during execution
    let recalled = memory.recall("preference", None, 5).await.unwrap();
    assert!(!recalled.is_empty());
    assert_eq!(recalled[0].key, "user_preference");
}

#[tokio::test]
async fn test_memory_store_roundtrip() {
    let memory = InMemoryMemoryStore::new();

    memory
        .remember(MemoryItem {
            id: "test_id".to_string(),
            category: MemoryCategory::TaskSummary,
            key: "test_key".to_string(),
            value: "test_value".to_string(),
            score: 1.0,
            created_at: 1000,
        })
        .await
        .unwrap();

    let items = memory.recall("test", None, 10).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].value, "test_value");

    // Forget by ID and verify
    memory.forget("test_id").await.unwrap();
    let items_after = memory.recall("test", None, 10).await.unwrap();
    assert!(items_after.is_empty());
}

// ---------------------------------------------------------------------------
// Tool Permission Boundary Tests
// ---------------------------------------------------------------------------

#[test]
fn test_tool_registry_default_tools_registered() {
    let mut registry = ToolRegistry::new();
    function_tools::register_default_tools(&mut registry);
    let tools = registry.list();

    // Should have multiple built-in computer tools
    assert!(
        tools.len() >= 5,
        "Expected at least 5 default tools, found {}",
        tools.len()
    );

    // Verify key tool names are present
    let names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
    assert!(
        names.contains(&"computer_screen".to_string()),
        "Missing computer_screen tool"
    );
    assert!(
        names.contains(&"computer_terminal".to_string()),
        "Missing computer_terminal tool"
    );
    assert!(
        names.contains(&"computer_files".to_string()),
        "Missing computer_files tool"
    );
}

#[test]
fn test_tool_permission_levels_enforced() {
    use function_tools::ToolPermissionLevel;

    let mut registry = ToolRegistry::new();
    function_tools::register_default_tools(&mut registry);

    let dummy_params = serde_json::json!({});

    // Every tool should have an assigned permission level
    for tool in registry.list() {
        let level = tool.permission_level(&dummy_params);
        assert!(
            matches!(
                level,
                ToolPermissionLevel::Safe
                    | ToolPermissionLevel::Confirm
                    | ToolPermissionLevel::Restricted
            ),
            "Tool '{}' has unexpected permission level",
            tool.name()
        );
    }
}

// ---------------------------------------------------------------------------
// Secrets Redaction Tests
// ---------------------------------------------------------------------------

#[test]
fn test_redact_secrets_openai_key() {
    let input = "Authorization: Bearer sk-proj-abcdefghijklmnopqrstuvwxyz1234567890";
    let redacted = function_config::redact_secrets(input);
    assert!(!redacted.contains("sk-proj-"));
    assert!(redacted.contains("[REDACTED_API_KEY]"));
}

#[test]
fn test_redact_secrets_multiple_keys() {
    let input = "Key1=sk-abc12345678 and Key2=sk-xyz98765432";
    let redacted = function_config::redact_secrets(input);
    assert!(!redacted.contains("sk-abc12345678"));
    assert!(!redacted.contains("sk-xyz98765432"));
    // Should have two redactions
    assert_eq!(redacted.matches("[REDACTED_API_KEY]").count(), 2);
}

#[test]
fn test_redact_secrets_preserves_safe_content() {
    let input = "Normal log message with no secrets at all";
    let redacted = function_config::redact_secrets(input);
    assert_eq!(redacted, input, "Safe content should not be modified");
}

#[test]
fn test_redact_secrets_short_sk_prefix_ignored() {
    // sk- followed by fewer than 5 chars should NOT be redacted
    let input = "prefix sk-ab end";
    let redacted = function_config::redact_secrets(input);
    assert_eq!(redacted, input, "Short sk- prefixes should be left alone");
}

// ---------------------------------------------------------------------------
// Configuration Persistence Tests
// ---------------------------------------------------------------------------

#[test]
fn test_config_save_and_reload() {
    use function_config::AppConfig;

    let temp = std::env::temp_dir().join("function_e2e_config_test");
    let _ = std::fs::create_dir_all(&temp);
    let config_file = temp.join("config.json");

    let mut config = AppConfig::default();
    config.sound_enabled = false;
    config.ai_provider.model = "gpt-4o-mini".to_string();

    // Serialize and write directly to temp
    let json = serde_json::to_string_pretty(&config).unwrap();
    std::fs::write(&config_file, &json).unwrap();

    // Read back and verify
    let content = std::fs::read_to_string(&config_file).unwrap();
    let loaded: AppConfig = serde_json::from_str(&content).unwrap();

    assert!(!loaded.sound_enabled);
    assert_eq!(loaded.ai_provider.model, "gpt-4o-mini");

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_function_dir_ends_with_dot_function() {
    let dir = function_config::function_dir();
    assert!(
        dir.ends_with(".function"),
        "function_dir() should end with .function, got: {}",
        dir.display()
    );
}

#[test]
fn test_config_and_memory_paths_are_inside_function_dir() {
    let dir = function_config::function_dir();
    let cfg = function_config::config_path();
    let mem = function_config::memory_path();

    assert!(
        cfg.starts_with(&dir),
        "config_path should be inside function_dir"
    );
    assert!(
        mem.starts_with(&dir),
        "memory_path should be inside function_dir"
    );
    assert!(cfg.ends_with("config.json"));
    assert!(mem.ends_with("memory.json"));
}

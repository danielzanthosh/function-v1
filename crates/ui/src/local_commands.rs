//! Local command recognition and resolution for Function.
//!
//! Intercepts commands locally BEFORE they reach the AI agent or require an API key.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommand {
    Configure,
    NewConversation,
    OpenConversations,
}

/// Resolve an explicit shell command prefix before AI dispatch.
pub fn resolve_shell_command(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    let command = trimmed.strip_prefix("> ")?.trim();
    (!command.is_empty()).then(|| command.to_string())
}

/// Resolve user input to a local command.
///
/// Recognizes (case-insensitive, ignoring leading and trailing whitespace):
/// - Settings: "settings", "configure", "preferences", "config"
/// - New Chat: "conversation new", "new chat", "new conversation", "clear chat"
/// - Conversations List: "conversation", "conversations", "history", "chats", "chat history"
///
/// Returns `Some(LocalCommand)` if recognized, or `None` so unrecognized input
/// continues through the AI agent pipeline.
pub fn resolve_local_command(input: &str) -> Option<LocalCommand> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if lower == "settings" || lower == "configure" || lower == "preferences" || lower == "config" {
        Some(LocalCommand::Configure)
    } else if lower == "conversation new"
        || lower == "new chat"
        || lower == "new conversation"
        || lower == "clear chat"
        || lower == "/new"
    {
        Some(LocalCommand::NewConversation)
    } else if lower == "conversation"
        || lower == "conversations"
        || lower == "history"
        || lower == "chats"
        || lower == "chat history"
        || lower == "/conversation"
        || lower == "/chats"
    {
        Some(LocalCommand::OpenConversations)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_settings_commands() {
        assert_eq!(
            resolve_local_command("settings"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("Settings"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("  SETTINGS  "),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("configure"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("Configure"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("preferences"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("Preferences"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("config"),
            Some(LocalCommand::Configure)
        );
        assert_eq!(
            resolve_local_command("Config"),
            Some(LocalCommand::Configure)
        );
    }

    #[test]
    fn test_resolve_conversation_commands() {
        assert_eq!(
            resolve_local_command("conversation"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("conversations"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("history"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("chats"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("conversation new"),
            Some(LocalCommand::NewConversation)
        );
        assert_eq!(
            resolve_local_command("new chat"),
            Some(LocalCommand::NewConversation)
        );
    }

    #[test]
    fn test_unrecognized_commands_flow_to_agent() {
        assert_eq!(resolve_local_command("open Safari"), None);
        assert_eq!(resolve_local_command("settings for wifi"), None);
        assert_eq!(resolve_local_command("configure my terminal"), None);
        assert_eq!(resolve_local_command(""), None);
    }

    #[test]
    fn test_resolve_explicit_shell_command_prefix() {
        assert_eq!(
            resolve_shell_command("> echo hi"),
            Some("echo hi".to_string())
        );
        assert_eq!(resolve_shell_command("  >  pwd"), Some("pwd".to_string()));
        assert_eq!(resolve_shell_command("> "), None);
        assert_eq!(resolve_shell_command("tell me about > shells"), None);
    }
}

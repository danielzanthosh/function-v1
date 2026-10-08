//! Local command recognition and resolution for Function.
//!
//! Intercepts commands locally BEFORE they reach the AI agent or require an API key.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommand {
    Configure,
    NewConversation,
    OpenConversations,
    ToggleTheme,
    ToggleVoice,
    ToggleSound,
    ClearInput,
    OpenMemory,
    OpenAbout,
    Help,
    SetModel(String),
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
/// - Settings: "settings", "configure", "preferences", "config", "/settings", "/configure"
/// - New Chat: "conversation new", "new chat", "new conversation", "clear chat", "/new"
/// - Conversations List: "conversation", "conversations", "history", "chats", "chat history", "/conversation", "/conversations", "/chats"
/// - Themes: "/themes", "/theme"
/// - Voice: "/voice"
/// - Sound: "/sound"
/// - Clear: "/clear"
/// - Memory: "/memory"
/// - About: "/about"
/// - Help: "/help"
/// - Model: "/model <name>"
///
/// Returns `Some(LocalCommand)` if recognized, or `None` so unrecognized input
/// continues through the AI agent pipeline.
pub fn resolve_local_command(input: &str) -> Option<LocalCommand> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if lower == "settings"
        || lower == "configure"
        || lower == "preferences"
        || lower == "config"
        || lower == "/settings"
        || lower == "/configure"
    {
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
        || lower == "/conversations"
        || lower == "/chats"
    {
        Some(LocalCommand::OpenConversations)
    } else if lower == "/themes" || lower == "/theme" {
        Some(LocalCommand::ToggleTheme)
    } else if lower == "/voice" {
        Some(LocalCommand::ToggleVoice)
    } else if lower == "/sound" {
        Some(LocalCommand::ToggleSound)
    } else if lower == "/clear" {
        Some(LocalCommand::ClearInput)
    } else if lower == "/memory" {
        Some(LocalCommand::OpenMemory)
    } else if lower == "/about" {
        Some(LocalCommand::OpenAbout)
    } else if lower == "/help" {
        Some(LocalCommand::Help)
    } else if lower == "/model" || lower.starts_with("/model ") {
        let arg = trimmed["/model".len()..].trim();
        Some(LocalCommand::SetModel(arg.to_string()))
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
            resolve_local_command("/settings"),
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
    }

    #[test]
    fn test_resolve_conversation_commands() {
        assert_eq!(
            resolve_local_command("conversation"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("/conversation"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("/conversations"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("history"),
            Some(LocalCommand::OpenConversations)
        );
        assert_eq!(
            resolve_local_command("/new"),
            Some(LocalCommand::NewConversation)
        );
    }

    #[test]
    fn test_resolve_slash_commands() {
        assert_eq!(
            resolve_local_command("/themes"),
            Some(LocalCommand::ToggleTheme)
        );
        assert_eq!(
            resolve_local_command("/voice"),
            Some(LocalCommand::ToggleVoice)
        );
        assert_eq!(
            resolve_local_command("/sound"),
            Some(LocalCommand::ToggleSound)
        );
        assert_eq!(
            resolve_local_command("/clear"),
            Some(LocalCommand::ClearInput)
        );
        assert_eq!(
            resolve_local_command("/memory"),
            Some(LocalCommand::OpenMemory)
        );
        assert_eq!(
            resolve_local_command("/about"),
            Some(LocalCommand::OpenAbout)
        );
        assert_eq!(
            resolve_local_command("/help"),
            Some(LocalCommand::Help)
        );
        assert_eq!(
            resolve_local_command("/model gpt-4o"),
            Some(LocalCommand::SetModel("gpt-4o".to_string()))
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

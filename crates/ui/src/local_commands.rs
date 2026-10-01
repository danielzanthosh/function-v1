//! Local command recognition and resolution for Function.
//!
//! Intercepts commands locally BEFORE they reach the AI agent or require an API key.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommand {
    Configure,
}

/// Resolve user input to a local command.
///
/// Recognizes (case-insensitive, ignoring leading and trailing whitespace):
/// - "Settings"
/// - "Configure"
/// - "Preferences"
/// - "Config"
///
/// Returns `Some(LocalCommand)` if recognized, or `None` so unrecognized input
/// continues through the AI agent pipeline.
pub fn resolve_local_command(input: &str) -> Option<LocalCommand> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("settings")
        || trimmed.eq_ignore_ascii_case("configure")
        || trimmed.eq_ignore_ascii_case("preferences")
        || trimmed.eq_ignore_ascii_case("config")
    {
        Some(LocalCommand::Configure)
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
    fn test_unrecognized_commands_flow_to_agent() {
        assert_eq!(resolve_local_command("open Safari"), None);
        assert_eq!(resolve_local_command("settings for wifi"), None);
        assert_eq!(resolve_local_command("configure my terminal"), None);
        assert_eq!(resolve_local_command(""), None);
    }
}

use crate::{ChatMessage, CompletionRequest, CompletionResponse, LlmProvider, MessageRole, ProviderError};
use async_trait::async_trait;
use serde_json::Value;
use std::process::{Command, Stdio};

/// GUI applications on macOS do not inherit the interactive shell's PATH.
/// Resolve Codex through the user's login shell and common package-manager
/// locations before reporting that it is unavailable.
pub fn resolve_codex_executable() -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("zsh").args(["-ilc", "command -v codex"]).output() {
            if output.status.success() {
                candidates.extend(String::from_utf8_lossy(&output.stdout).lines().filter_map(|path| {
                    let path = std::path::PathBuf::from(path.trim());
                    path.is_file().then_some(path)
                }));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::PathBuf::from(home);
            candidates.extend([
                home.join(".npm-global/bin/codex"),
                home.join(".local/bin/codex"),
                home.join(".volta/bin/codex"),
            ]);
        }
        candidates.extend([
            std::path::PathBuf::from("/opt/homebrew/bin/codex"),
            std::path::PathBuf::from("/usr/local/bin/codex"),
        ]);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("where.exe").arg("codex").output() {
            if output.status.success() {
                candidates.extend(String::from_utf8_lossy(&output.stdout).lines().filter_map(|path| {
                    let path = std::path::PathBuf::from(path.trim());
                    path.is_file().then_some(path)
                }));
            }
        }
        if let Some(app_data) = std::env::var_os("APPDATA") {
            candidates.push(std::path::PathBuf::from(app_data).join("npm/codex.cmd"));
        }
    }
    candidates.push(std::path::PathBuf::from("codex"));
    candidates.into_iter().find(|candidate| candidate.to_string_lossy() == "codex" || candidate.is_file())
}

/// Windows cannot execute a `.cmd` shim directly with CreateProcess.
pub fn codex_command(executable: std::path::PathBuf) -> Command {
    #[cfg(target_os = "windows")]
    if executable.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
    }) {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/S", "/C"]);
        command.arg(executable);
        return command;
    }
    Command::new(executable)
}

/// Uses the locally authenticated Codex CLI session. The OAuth credential is
/// never read or copied by Function; Codex owns its storage and renewal.
pub struct CodexChatGptProvider {
    default_model: String,
}

impl CodexChatGptProvider {
    pub fn new(default_model: impl Into<String>) -> Self {
        Self { default_model: default_model.into() }
    }
}

fn prompt_from_messages(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|message| {
            let role = match message.role {
                MessageRole::System => "System",
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::Tool => "Tool result",
            };
            format!("{role}: {}", message.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn codex_execution_instructions() -> &'static str {
    if cfg!(target_os = "macos") {
        "\n\nCodex execution mode: You are running locally with YOLO permissions. Complete the user's task autonomously instead of replying with a limitation. Use the available shell and macOS automation commands (including osascript/System Events, open, and screencapture) to control applications, mouse, keyboard, and browser windows when needed. Observe the result, recover from errors, and continue until the task is complete. Do not claim that desktop controls are unavailable unless an actual command was attempted and failed. Never stop after merely describing what the user could do."
    } else if cfg!(target_os = "windows") {
        "\n\nCodex execution mode: You are running locally with YOLO permissions. Complete the user's task autonomously instead of replying with a limitation. Use PowerShell, native Windows automation, and available command-line tools to control applications, mouse, keyboard, and browser windows when needed. Observe the result, recover from errors, and continue until the task is complete. Do not claim that desktop controls are unavailable unless an actual command was attempted and failed. Never stop after merely describing what the user could do."
    } else {
        "\n\nCodex execution mode: You are running locally with YOLO permissions. Complete the user's task autonomously using available native shell and desktop automation commands. Observe results, recover from errors, and continue until complete. Do not stop after merely describing what the user could do."
    }
}

fn run_codex(model: String, prompt: String) -> Result<String, ProviderError> {
    let executable = resolve_codex_executable().ok_or_else(|| ProviderError::NotConfigured("Codex CLI was not found. Install it or add its directory to your login shell PATH.".into()))?;
    let output = codex_command(executable)
        .args([
            "exec",
            "--json",
            "--ephemeral",
            "--dangerously-bypass-approvals-and-sandbox",
            "--skip-git-repo-check",
            "--model",
            &model,
            "--",
            &prompt,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| ProviderError::NotConfigured(format!("Codex CLI is unavailable: {error}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut answer = String::new();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else { continue };
        if event.get("type").and_then(Value::as_str) == Some("item.completed") {
            let item = event.get("item").unwrap_or(&Value::Null);
            if item.get("type").and_then(Value::as_str) == Some("agent_message") {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    if !answer.is_empty() {
                        answer.push('\n');
                    }
                    answer.push_str(text);
                }
            }
        }
    }
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ProviderError::Api { code: output.status.code().unwrap_or(1) as u16, message: if details.is_empty() { "Codex request failed".into() } else { details } });
    }
    if answer.is_empty() {
        return Err(ProviderError::Api { code: 502, message: "Codex returned no assistant message".into() });
    }
    Ok(answer)
}

#[async_trait]
impl LlmProvider for CodexChatGptProvider {
    fn name(&self) -> &str { "chatgpt-plan" }
    fn context_limit(&self, _model: &str) -> usize { 128_000 }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let model = if req.model.is_empty() || req.model == "default" { self.default_model.clone() } else { req.model };
        let prompt = format!("{}{}", prompt_from_messages(&req.messages), codex_execution_instructions());
        let content = tokio::task::spawn_blocking(move || run_codex(model, prompt))
            .await
            .map_err(|error| ProviderError::Network(format!("Codex task failed: {error}")))??;
        Ok(CompletionResponse { message: ChatMessage::assistant(content), finish_reason: Some("stop".into()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_uses_local_chatgpt_session_and_preserves_message_roles() {
        let provider = CodexChatGptProvider::new("gpt-5");
        assert_eq!(provider.name(), "chatgpt-plan");
        let prompt = prompt_from_messages(&[
            ChatMessage::system("Follow the task"),
            ChatMessage::user("Hello"),
        ]);
        assert!(prompt.contains("System: Follow the task"));
        assert!(prompt.contains("User: Hello"));
    }
}


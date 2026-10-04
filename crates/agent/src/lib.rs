//! Core AI Agent loop and state orchestration.
//!
//! Executes the Observe-Think-Act cycle. Converts natural language instructions
//! into structured tool calls, observes real system feedback, and reports state
//! changes cleanly to the UI layer without coupling to visual views.

use function_memory::MemoryStore;
use function_providers::{ChatMessage, CompletionRequest, LlmProvider, ToolDefinition};
use function_tools::{ToolContext, ToolRegistry, ToolResult};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tokio::sync::broadcast;

pub mod context;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Tool error: {0}")]
    Tool(String),
    #[error("Task canceled by user")]
    Canceled,
    #[error("Execution step limit exceeded")]
    StepLimitExceeded,
}

/// Agent lifecycle states observed by the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum AgentState {
    Idle,
    Listening,
    Processing { thought_summary: Option<String> },
    Streaming { chunk: String, accumulated: String },
    Acting { action_description: String },
    WaitingForConfirmation { action: String, details: String },
    Completed { summary: String, new_history: Vec<ChatMessage> },
    Error { message: String },
}

impl PartialEq for AgentState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AgentState::Idle, AgentState::Idle) => true,
            (AgentState::Listening, AgentState::Listening) => true,
            (
                AgentState::Processing { thought_summary: a },
                AgentState::Processing { thought_summary: b },
            ) => a == b,
            (
                AgentState::Streaming { accumulated: a, .. },
                AgentState::Streaming { accumulated: b, .. },
            ) => a == b,
            (
                AgentState::Acting { action_description: a },
                AgentState::Acting { action_description: b },
            ) => a == b,
            (
                AgentState::WaitingForConfirmation { action: a, details: c },
                AgentState::WaitingForConfirmation { action: b, details: d },
            ) => a == b && c == d,
            // Compare only the summary; history is not used for equality checks
            (AgentState::Completed { summary: a, .. }, AgentState::Completed { summary: b, .. }) => a == b,
            (AgentState::Error { message: a }, AgentState::Error { message: b }) => a == b,
            _ => false,
        }
    }
}

impl Eq for AgentState {}

/// Detailed action event emitted during agent operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActionEvent {
    pub step: usize,
    pub description: String,
    pub success: bool,
}

/// Core agent orchestrator.
pub struct Agent {
    provider: RwLock<Arc<dyn LlmProvider>>,
    tools: ToolRegistry,
    memory: Arc<dyn MemoryStore>,
    state_tx: broadcast::Sender<AgentState>,
    cancel_requested: Arc<std::sync::atomic::AtomicBool>,
    max_steps: usize,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        memory: Arc<dyn MemoryStore>,
    ) -> Self {
        let (state_tx, _) = broadcast::channel(32);
        Self {
            provider: RwLock::new(provider),
            tools,
            memory,
            state_tx,
            cancel_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            max_steps: 15,
        }
    }

    /// Request cancellation of the currently executing task.
    pub fn cancel(&self) {
        self.cancel_requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Check if cancellation was requested.
    pub fn is_canceled(&self) -> bool {
        self.cancel_requested
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Replace the active provider without rebuilding the agent or losing its
    /// tools, memory, state subscription, or cancellation state.
    pub fn set_provider(&self, provider: Arc<dyn LlmProvider>) {
        if let Ok(mut active_provider) = self.provider.write() {
            *active_provider = provider;
        }
    }

    /// Subscribe to state updates (used by GPUI views).
    pub fn subscribe_state(&self) -> broadcast::Receiver<AgentState> {
        self.state_tx.subscribe()
    }

    fn update_state(&self, state: AgentState) {
        let _ = self.state_tx.send(state);
    }

    /// Execute a task given a user prompt.
    pub async fn execute_task(&self, user_prompt: &str) -> Result<String, AgentError> {
        let (reply, _) = self.execute_with_history(user_prompt, vec![]).await?;
        Ok(reply)
    }

    /// Execute with persistent chat history.
    ///
    /// `prior_history` contains only the User/Assistant turn pairs from previous exchanges
    /// (no system message — this function prepends one). Returns the assistant reply text
    /// and the updated history to store for the next turn.
    pub async fn execute_with_history(
        &self,
        user_prompt: &str,
        prior_history: Vec<ChatMessage>,
    ) -> Result<(String, Vec<ChatMessage>), AgentError> {
        self.cancel_requested
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.update_state(AgentState::Processing {
            thought_summary: None,
        });

        // Retrieve relevant memory items
        let context_items = self
            .memory
            .recall(user_prompt, None, 5)
            .await
            .unwrap_or_default();

        let mut system_prompt = "\
You are Function, a fast, proactive, native agentic desktop AI assistant.

Operating System Awareness & Coordinate Space:
- You run directly on the host computer. Detect the OS and interact using native capabilities.
- NEVER assume Windows when running on macOS or Linux, and vice-versa.
- NEVER use Windows commands like `start chrome.exe` or `dir` on macOS or Linux.
- Coordinates for mouse/keyboard tools (mouse_move, click, double_click, etc.) use LOGICAL screen coordinates, NOT raw Retina pixels.
- Screen screenshots return physical size, logical size, scale factor, and a 10x10 virtual grid overlay to help identify logical coordinates accurately.

Primary Desktop Capabilities & Tools:
1. `open_app`: Launch or focus applications natively by name.
2. `close_app`: Gracefully quit or terminate applications by name.
3. `take_screenshot`: Capture current screen. Use when you need visual context or to verify UI state.
4. `mouse_move(x, y)` / `click(x, y)` / `double_click(x, y)` / `mouse_scroll(delta)`: Granular mouse control.
5. `type_text(text)` / `key_press(key)` / `hotkey(keys)`: Granular keyboard control.
6. `execute_command`: Run shell commands when no native tool exists.
7. `fs`: Create, read, edit, or list files/folders.

Agentic Control Flow (DO NOT BLINDLY SPAM ACTIONS):
- User Request -> Model decides if screen interaction is needed.
- If screen interaction is needed:
  1. Take screenshot -> Observe UI state and virtual grid coordinates.
  2. Choose a deliberate action (e.g. click, type, hotkey).
  3. Take screenshot again when verification is useful.
  4. Continue until objective is accomplished.
- Read tool results and errors carefully before deciding the next step.

macOS Permissions Guidance:
- If a tool fails with permission errors (e.g., Accessibility or Screen Recording denied):
  Prompt the user clearly to enable:
  `System Settings -> Privacy & Security -> Accessibility / Screen Recording -> Function`.

Sandbox & Filesystem Scope:
- By default, perform file and folder operations within `~/Desktop/Function Sandbox` unless explicitly requested otherwise."
            .to_string();

        if !context_items.is_empty() {
            system_prompt.push_str("\n\nRelevant Context:\n");
            for item in context_items {
                system_prompt.push_str(&format!("- {}: {}\n", item.key, item.value));
            }
        }

        let tool_definitions: Vec<ToolDefinition> = self
            .tools
            .list()
            .into_iter()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect();

        // Build the full message list: system + prior history + new user turn
        let mut messages = Vec::with_capacity(prior_history.len() + 2);
        messages.push(ChatMessage::system(system_prompt));
        messages.extend(prior_history.clone());
        messages.push(ChatMessage::user(user_prompt));

        // history_tail tracks just the conversation turns (no system msg, no tool internals)
        // so the caller can persist and pass them back next time.
        let mut history_tail = prior_history;
        history_tail.push(ChatMessage::user(user_prompt));

        let mut step = 0;
        let mut final_result = String::new();

        while step < self.max_steps {
            if self.is_canceled() {
                self.update_state(AgentState::Idle);
                return Err(AgentError::Canceled);
            }
            step += 1;

            let provider = self
                .provider
                .read()
                .map_err(|e| AgentError::Provider(format!("Provider lock poisoned: {e}")))?
                .clone();
            let context_budget = context::ContextBudget {
                context_limit: provider.context_limit("default"),
                output_reserve: 4_096,
            };
            let (request_messages, compaction) = context::compact_messages(&messages, context_budget);
            if compaction.before_tokens != compaction.after_tokens {
                tracing::info!(
                    provider = %provider.name(),
                    before_tokens = compaction.before_tokens,
                    after_tokens = compaction.after_tokens,
                    removed_messages = compaction.removed_messages,
                    removed_images = compaction.removed_images,
                    truncated_outputs = compaction.truncated_outputs,
                    "Compacted model context before dispatch"
                );
            }

            let req = CompletionRequest {
                model: "default".to_string(),
                messages: request_messages,
                tools: tool_definitions.clone(),
                temperature: Some(0.7),
            };

            let state_tx_clone = self.state_tx.clone();
            let mut stream_accumulated = String::new();
            let response = provider
                .complete_stream(
                    req,
                    Box::new(move |token: String| {
                        stream_accumulated.push_str(&token);
                        let _ = state_tx_clone.send(AgentState::Streaming {
                            chunk: token,
                            accumulated: stream_accumulated.clone(),
                        });
                    }),
                )
                .await
                .map_err(|e| {
                    let err = AgentError::Provider(e.to_string());
                    self.update_state(AgentState::Error {
                        message: err.to_string(),
                    });
                    err
                })?;

            if self.is_canceled() {
                self.update_state(AgentState::Idle);
                return Err(AgentError::Canceled);
            }

            let response_msg = response.message;

            if let Some(tool_calls) = response_msg.tool_calls.clone() {
                if tool_calls.is_empty() {
                    final_result = response_msg.content.clone();
                    messages.push(response_msg);
                    break;
                }

                messages.push(response_msg);

                for call in &tool_calls {
                    if self.is_canceled() {
                        self.update_state(AgentState::Idle);
                        return Err(AgentError::Canceled);
                    }

                    // Structured debug logging: agent decision → selected tool
                    tracing::info!(
                        target: "function_agent",
                        step = step,
                        tool = %call.name,
                        arguments = %call.arguments,
                        "agent decision → selected tool: {} with args: {}",
                        call.name,
                        call.arguments
                    );

                    let action_desc = match call.name.as_str() {
                        // High-level OS-aware tools
                        "open_app" => {
                            let app_name = call
                                .arguments
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("application");
                            format!("Opening {}...", app_name)
                        }
                        "close_app" => {
                            let app_name = call
                                .arguments
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("application");
                            format!("Closing {}...", app_name)
                        }
                        "take_screenshot" => "Capturing screen observation...".to_string(),
                        "click" => {
                            if let (Some(x), Some(y)) = (
                                call.arguments.get("x").and_then(|v| v.as_f64()),
                                call.arguments.get("y").and_then(|v| v.as_f64()),
                            ) {
                                format!("Clicking at ({}, {})...", x as i32, y as i32)
                            } else {
                                "Clicking target...".to_string()
                            }
                        }
                        "double_click" => "Double-clicking target...".to_string(),
                        "type_text" => "Typing text...".to_string(),
                        "press_key" => {
                            let key = call
                                .arguments
                                .get("key")
                                .and_then(|k| k.as_str())
                                .unwrap_or("key");
                            format!("Pressing {}...", key)
                        }
                        "scroll" => "Scrolling view...".to_string(),
                        "execute_command" => {
                            let cmd = call
                                .arguments
                                .get("command")
                                .and_then(|c| c.as_str())
                                .unwrap_or("command");
                            format!("Executing `{}`...", cmd)
                        }
                        // Low-level / legacy tools
                        "computer_screen" => {
                            let action = call
                                .arguments
                                .get("action")
                                .and_then(|a| a.as_str())
                                .unwrap_or("");
                            match action {
                                "screenshot" => "Capturing screen context...".to_string(),
                                "dimensions" => "Checking screen resolution...".to_string(),
                                "cursor" => "Locating mouse cursor...".to_string(),
                                _ => "Reading screen info...".to_string(),
                            }
                        }
                        "computer_mouse" => {
                            let action = call
                                .arguments
                                .get("action")
                                .and_then(|a| a.as_str())
                                .unwrap_or("");
                            match action {
                                "move" => "Moving mouse...".to_string(),
                                "click" => "Clicking mouse...".to_string(),
                                "double_click" => "Double-clicking...".to_string(),
                                "right_click" => "Right-clicking...".to_string(),
                                "middle_click" => "Middle-clicking...".to_string(),
                                "drag" => "Dragging on screen...".to_string(),
                                "scroll" => "Scrolling...".to_string(),
                                _ => "Operating mouse...".to_string(),
                            }
                        }
                        "computer_keyboard" => {
                            let action = call
                                .arguments
                                .get("action")
                                .and_then(|a| a.as_str())
                                .unwrap_or("");
                            match action {
                                "type" => "Typing text...".to_string(),
                                "key" => "Pressing key...".to_string(),
                                "shortcut" => "Triggering shortcut...".to_string(),
                                _ => "Sending keystrokes...".to_string(),
                            }
                        }
                        "computer_apps" => {
                            let action = call
                                .arguments
                                .get("action")
                                .and_then(|a| a.as_str())
                                .unwrap_or("");
                            match action {
                                "open" => {
                                    let app_name = call
                                        .arguments
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("application");
                                    format!("Opening {}...", app_name)
                                }
                                "list" => "Checking running applications...".to_string(),
                                "focus" => "Switching window focus...".to_string(),
                                _ => "Managing applications...".to_string(),
                            }
                        }
                        "fs" => "Accessing filesystem...".to_string(),
                        "terminal" => "Executing terminal command...".to_string(),
                        _ => format!("Running {}", call.name),
                    };

                    self.update_state(AgentState::Acting {
                        action_description: action_desc,
                    });

                    let ctx = ToolContext {
                        session_id: "default".to_string(),
                        // Function is configured as a fully agentic assistant.
                        // Tool implementations still enforce RESTRICTED actions,
                        // but confirmation-gated actions are allowed after the
                        // user explicitly enabled maximum agentic permissions.
                        allow_sensitive: true,
                    };

                    // Structured debug logging: execution
                    tracing::info!(
                        target: "function_agent",
                        tool = %call.name,
                        "execution: running tool '{}'...",
                        call.name
                    );

                    let result = match self
                        .tools
                        .execute(&call.name, call.arguments.clone(), &ctx)
                        .await
                    {
                        Ok(res) => res,
                        Err(function_tools::ToolError::RequiresConfirmation) => {
                            self.update_state(AgentState::WaitingForConfirmation {
                                action: call.name.clone(),
                                details: format!("{}", call.arguments),
                            });
                            ToolResult::failure(
                                format!("Action '{}' requires user confirmation", call.name),
                                "Action paused: Operation requires explicit user confirmation before executing.",
                            )
                        }
                        Err(e) => ToolResult::failure(
                            format!("Failed to run {}", call.name),
                            e.to_string(),
                        ),
                    };

                    // Structured debug logging: result
                    tracing::info!(
                        target: "function_agent",
                        tool = %call.name,
                        success = result.success,
                        summary = %result.summary,
                        "result: {} -> success={}, summary='{}'",
                        call.name,
                        result.success,
                        result.summary
                    );

                    // Extract screenshot base64 if visual observation was captured
                    let mut screenshot_base64 = None;
                    if call.name == "take_screenshot" || call.name == "computer_screen" {
                        if let Some(b64) = result.output.get("base64").and_then(|v| v.as_str()) {
                            screenshot_base64 = Some(b64.to_string());
                        }
                    }

                    // Structured debug logging: observation
                    if let Some(ref b64) = screenshot_base64 {
                        tracing::info!(
                            target: "function_agent",
                            tool = %call.name,
                            observation = "visual screenshot captured",
                            "observation: visual screen state captured (bytes: {})",
                            b64.len()
                        );
                    } else {
                        tracing::info!(
                            target: "function_agent",
                            tool = %call.name,
                            output = %result.output,
                            "observation: tool output recorded for next step"
                        );
                    }

                    messages.push(ChatMessage::tool(
                        call.id.clone(),
                        result.output.to_string(),
                    ));

                    if let Some(b64) = screenshot_base64 {
                        messages.push(ChatMessage::user_with_images(
                            "Visual screen observation:",
                            vec![b64],
                        ));
                    }

                    // Structured debug logging: next action
                    tracing::info!(
                        target: "function_agent",
                        step = step,
                        "next action: evaluating progress and deciding next step"
                    );
                }

                self.update_state(AgentState::Processing {
                    thought_summary: Some("Evaluating progress...".to_string()),
                });
            } else {
                final_result = response_msg.content.clone();
                messages.push(response_msg);
                break;
            }
        }

        if step >= self.max_steps && final_result.is_empty() {
            let err = AgentError::StepLimitExceeded;
            self.update_state(AgentState::Error {
                message: err.to_string(),
            });
            return Err(err);
        }

        // Append the assistant reply to the history tail for the caller to store
        if !final_result.is_empty() {
            history_tail.push(ChatMessage::assistant(final_result.clone()));
        }

        self.update_state(AgentState::Completed {
            summary: final_result.clone(),
            new_history: history_tail.clone(),
        });
        Ok((final_result, history_tail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use function_memory::InMemoryMemoryStore;
    use function_providers::MockLlmProvider;

    #[tokio::test]
    async fn test_agent_execution() {
        let provider = Arc::new(MockLlmProvider::new("Task completed successfully"));
        let tools = ToolRegistry::new();
        let memory = Arc::new(InMemoryMemoryStore::new());
        let agent = Agent::new(provider, tools, memory);

        let mut rx = agent.subscribe_state();
        let result = agent.execute_task("Check status").await.unwrap();
        assert_eq!(result, "Task completed successfully");

        let state = rx.recv().await.unwrap();
        assert!(matches!(state, AgentState::Processing { .. }));
    }

    #[tokio::test]
    async fn test_agent_can_replace_provider_at_runtime() {
        let initial = Arc::new(MockLlmProvider::new("initial response"));
        let replacement = Arc::new(MockLlmProvider::new("replacement response"));
        let tools = ToolRegistry::new();
        let memory = Arc::new(InMemoryMemoryStore::new());
        let agent = Agent::new(initial, tools, memory);

        agent.set_provider(replacement);

        let response = agent.execute_task("hello").await.unwrap();
        assert_eq!(response, "replacement response");
    }
}

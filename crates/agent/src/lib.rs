//! Core AI Agent loop and state orchestration.
//!
//! Executes the Observe-Think-Act cycle. Converts natural language instructions
//! into structured tool calls, observes real system feedback, and reports state
//! changes cleanly to the UI layer without coupling to visual views.

use function_memory::MemoryStore;
use function_providers::{ChatMessage, CompletionRequest, LlmProvider, ToolDefinition};
use function_tools::{ToolContext, ToolRegistry, ToolResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;

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
    provider: Arc<dyn LlmProvider>,
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
            provider,
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
You are Function, a fast, proactive, agentic desktop AI assistant running natively on this computer.

Core Capabilities:
1. Conversational & Knowledge: Answer questions, explain concepts, write text, code, brainstorm, and converse naturally. Use clean GitHub markdown.
2. Screen Understanding & Vision: If the user asks about something on their screen (e.g., \"What's this?\", \"What error is showing?\", \"Where do I click?\", \"Analyze what's on my display\"), call `computer_screen` with action `screenshot` to capture visual context. Then inspect the screen image and answer accurately. Do NOT take screenshots if visual context is not needed.
3. Computer Interaction: You have full native desktop tools to interact with apps, mouse, keyboard, and terminal:
   - `computer_screen`: capture screenshots, retrieve display dimensions, find cursor position.
   - `computer_mouse`: move mouse cursor, click, double click, right click, middle click, drag, scroll.
   - `computer_keyboard`: type text, press keys (return, tab, space, escape, etc.), send keyboard shortcuts (e.g. ctrl+c, command+space).
   - `computer_apps`: open applications by name (e.g. Chrome, Terminal, VS Code, Finder), list active windows, focus windows.
   - `fs`: read/write/list files.
   - `terminal`: execute commands safely.

Agentic Execution Loop:
- For complex desktop tasks, reason step-by-step: understand intent -> observe state or screen -> decide next tool -> inspect output -> repeat until task is accomplished -> provide a concise final summary.
- Always provide a concise, helpful summary when finished. Never dump raw internal tool output to the user."
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

            let req = CompletionRequest {
                model: "default".to_string(),
                messages: messages.clone(),
                tools: tool_definitions.clone(),
                temperature: Some(0.7),
            };

            let state_tx_clone = self.state_tx.clone();
            let mut stream_accumulated = String::new();
            let response = self
                .provider
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

                    let action_desc = match call.name.as_str() {
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
                        allow_sensitive: false,
                    };

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

                    let mut screenshot_base64 = None;
                    if call.name == "computer_screen" {
                        if let Some(b64) = result.output.get("base64").and_then(|v| v.as_str()) {
                            screenshot_base64 = Some(b64.to_string());
                        }
                    }

                    messages.push(ChatMessage::tool(
                        call.id.clone(),
                        result.output.to_string(),
                    ));

                    if let Some(b64) = screenshot_base64 {
                        messages.push(ChatMessage::user_with_images(
                            "Visual screen capture:",
                            vec![b64],
                        ));
                    }
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
}

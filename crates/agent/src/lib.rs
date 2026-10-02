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
            max_steps: 15,
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
You are Function, a smart desktop AI assistant — similar to Siri or Copilot but running natively on this computer.

Your primary role is to be a helpful conversational AI:
- Answer questions, explain concepts, write text, brainstorm, and help with any intellectual task directly in your response.
- Engage in natural back-and-forth conversation; remember the chat history provided.
- Only use the registered computer-control tools (screen, mouse, keyboard, apps, files, terminal) when the user's request genuinely requires controlling the computer (e.g. \"open Chrome\", \"type this in VS Code\", \"take a screenshot\").
- For conversational, informational, or creative requests, respond with clear natural-language text — do NOT call tools just because they exist.
- Be concise but thorough. Use markdown formatting where it adds clarity (lists, bold, code blocks).
- Do not claim you cannot do things that you can answer conversationally."
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
            step += 1;

            let req = CompletionRequest {
                model: "default".to_string(),
                messages: messages.clone(),
                tools: tool_definitions.clone(),
                temperature: Some(0.7),
            };

            let response = self.provider.complete(req).await.map_err(|e| {
                let err = AgentError::Provider(e.to_string());
                self.update_state(AgentState::Error {
                    message: err.to_string(),
                });
                err
            })?;

            let response_msg = response.message;

            if let Some(tool_calls) = response_msg.tool_calls.clone() {
                if tool_calls.is_empty() {
                    final_result = response_msg.content.clone();
                    messages.push(response_msg);
                    break;
                }

                messages.push(response_msg);

                for call in &tool_calls {
                    self.update_state(AgentState::Acting {
                        action_description: format!("Executing {}", call.name),
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

                    messages.push(ChatMessage::tool(
                        call.id.clone(),
                        result.output.to_string(),
                    ));
                }

                self.update_state(AgentState::Processing {
                    thought_summary: Some("Evaluating tool output".to_string()),
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

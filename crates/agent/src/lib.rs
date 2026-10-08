//! Core AI Agent loop and state orchestration.
//!
//! Executes the Observe-Think-Act cycle. Converts natural language instructions
//! into structured tool calls, observes real system feedback, and reports state
//! changes cleanly to the UI layer without coupling to visual views.

use function_memory::MemoryStore;
use function_providers::{
    ChatMessage, CompletionRequest, LlmProvider, RetryingLlmProvider, ToolDefinition,
};
use function_tools::{ToolContext, ToolRegistry, ToolResult};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{broadcast, watch};

pub mod context;

/// `tracing` target for request lifecycle events. Filter with
/// `RUST_LOG=function_agent::lifecycle=info` to diagnose stuck requests.
pub const LIFECYCLE_LOG_TARGET: &str = "function_agent::lifecycle";

const STATE_CHANNEL_CAPACITY: usize = 256;
const MEMORY_RECALL_TIMEOUT: Duration = Duration::from_secs(5);
const TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(180);

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
    #[error("Repeated tool call loop detected: {0}")]
    RepeatedToolCalls(String),
}

/// Agent lifecycle states observed by the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum AgentState {
    Idle,
    Listening,
    Processing {
        thought_summary: Option<String>,
    },
    Streaming {
        chunk: String,
        accumulated: String,
    },
    Acting {
        action_description: String,
    },
    WaitingForConfirmation {
        action: String,
        details: String,
    },
    Completed {
        summary: String,
        new_history: Vec<ChatMessage>,
    },
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_history: Option<Vec<ChatMessage>>,
    },
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
                AgentState::Acting {
                    action_description: a,
                },
                AgentState::Acting {
                    action_description: b,
                },
            ) => a == b,
            (
                AgentState::WaitingForConfirmation {
                    action: a,
                    details: c,
                },
                AgentState::WaitingForConfirmation {
                    action: b,
                    details: d,
                },
            ) => a == b && c == d,
            // Compare only the summary; history is not used for equality checks
            (
                AgentState::Completed { summary: a, .. },
                AgentState::Completed { summary: b, .. },
            ) => a == b,
            (AgentState::Error { message: a, .. }, AgentState::Error { message: b, .. }) => a == b,
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
    /// Monotonic cancellation generation. Every `cancel()` and every new run
    /// bumps it; a run is canceled once the generation differs from the one it
    /// started with. Unlike a resettable flag, a new run can never "un-cancel"
    /// a previous run that is still in flight.
    cancel_tx: watch::Sender<u64>,
    current_run_cancel_base: AtomicU64,
    run_counter: AtomicU64,
    active_run: Arc<AtomicU64>,
    max_steps: usize,
    request_delay_ms: AtomicU64,
    input_token_limit: RwLock<function_config::InputTokenLimit>,
    context_optimization_enabled: RwLock<bool>,
}

/// Per-run handle used to emit state updates and observe cancellation.
#[derive(Clone)]
struct RunHandle {
    run_id: u64,
    cancel_base: u64,
    cancel_rx: watch::Receiver<u64>,
    state_tx: broadcast::Sender<AgentState>,
    active_run: Arc<AtomicU64>,
}

impl RunHandle {
    fn is_canceled(&self) -> bool {
        *self.cancel_rx.borrow() != self.cancel_base
    }

    fn is_current(&self) -> bool {
        self.active_run.load(Ordering::SeqCst) == self.run_id
    }

    /// Emit an intermediate state; dropped once the run is canceled so a
    /// stale run cannot overwrite the UI state of a newer request.
    fn emit(&self, state: AgentState) {
        if !self.is_canceled() {
            let _ = self.state_tx.send(state);
        }
    }

    /// Emit a terminal state unless a newer run has superseded this one.
    fn emit_final(&self, state: AgentState) {
        if self.is_current() {
            let _ = self.state_tx.send(state);
        }
    }

    /// Resolves once this run has been canceled or superseded.
    async fn canceled(&self) {
        let mut rx = self.cancel_rx.clone();
        loop {
            if *rx.borrow_and_update() != self.cancel_base {
                return;
            }
            if rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

/// Guarantees a terminal state is broadcast even if the run future panics or
/// is dropped before reaching one, so the UI never stays in a busy state.
struct RunGuard {
    run: RunHandle,
    started: Instant,
    armed: bool,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        if self.run.is_canceled() {
            tracing::info!(
                target: LIFECYCLE_LOG_TARGET,
                run_id = self.run.run_id,
                elapsed_ms,
                "request dropped after cancellation"
            );
            self.run.emit_final(AgentState::Idle);
        } else {
            tracing::error!(
                target: LIFECYCLE_LOG_TARGET,
                run_id = self.run.run_id,
                elapsed_ms,
                panicking = std::thread::panicking(),
                "request ended without reaching a terminal state"
            );
            self.run.emit_final(AgentState::Error {
                message: "The request ended unexpectedly. Please try again.".to_string(),
                new_history: None,
            });
        }
    }
}

struct RunFailure {
    error: AgentError,
    history: Vec<ChatMessage>,
}

impl RunFailure {
    fn new(error: AgentError, history: &[ChatMessage]) -> Self {
        Self {
            error,
            history: history.to_vec(),
        }
    }
}

fn describe_tool_call(call: &function_providers::ToolCall) -> String {
    match call.name.as_str() {
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
    }
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        memory: Arc<dyn MemoryStore>,
    ) -> Self {
        let (state_tx, _) = broadcast::channel(STATE_CHANNEL_CAPACITY);
        let (cancel_tx, _) = watch::channel(0);
        Self {
            provider: RwLock::new(provider),
            tools,
            memory,
            state_tx,
            cancel_tx,
            current_run_cancel_base: AtomicU64::new(0),
            run_counter: AtomicU64::new(0),
            active_run: Arc::new(AtomicU64::new(0)),
            max_steps: 15,
            request_delay_ms: AtomicU64::new(0),
            input_token_limit: RwLock::new(function_config::InputTokenLimit::Auto),
            context_optimization_enabled: RwLock::new(true),
        }
    }

    /// Set a request delay in milliseconds applied before sending new requests to the provider.
    pub fn set_request_delay(&self, delay_ms: u64) {
        self.request_delay_ms.store(delay_ms, Ordering::Relaxed);
    }

    /// Set the input token budget limit setting.
    pub fn set_input_token_limit(&self, limit: function_config::InputTokenLimit) {
        if let Ok(mut lock) = self.input_token_limit.write() {
            *lock = limit;
        }
    }

    /// Enable or disable automatic context compaction before provider requests.
    pub fn set_context_optimization_enabled(&self, enabled: bool) {
        if let Ok(mut lock) = self.context_optimization_enabled.write() {
            *lock = enabled;
        }
    }

    /// Request cancellation of the currently executing task. In-flight
    /// provider requests, request delays, and tool calls are abandoned
    /// immediately rather than at the next step boundary.
    pub fn cancel(&self) {
        self.cancel_tx
            .send_modify(|generation| *generation = generation.wrapping_add(1));
        tracing::info!(
            target: LIFECYCLE_LOG_TARGET,
            run_id = self.active_run.load(Ordering::SeqCst),
            "cancellation requested"
        );
    }

    /// Check if cancellation was requested for the most recent run.
    pub fn is_canceled(&self) -> bool {
        *self.cancel_tx.borrow() != self.current_run_cancel_base.load(Ordering::SeqCst)
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    pub fn set_max_steps(&mut self, max_steps: usize) {
        self.max_steps = max_steps;
    }

    pub fn max_steps(&self) -> usize {
        self.max_steps
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

    fn begin_run(&self) -> RunHandle {
        // A new run supersedes (and cancels) any run still in flight.
        let mut cancel_base = 0;
        self.cancel_tx.send_modify(|generation| {
            *generation = generation.wrapping_add(1);
            cancel_base = *generation;
        });
        self.current_run_cancel_base
            .store(cancel_base, Ordering::SeqCst);
        let run_id = self.run_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.active_run.fetch_max(run_id, Ordering::SeqCst);
        RunHandle {
            run_id,
            cancel_base,
            cancel_rx: self.cancel_tx.subscribe(),
            state_tx: self.state_tx.clone(),
            active_run: self.active_run.clone(),
        }
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
    ///
    /// Every exit path broadcasts exactly one terminal state (`Completed`,
    /// `Error`, or `Idle` on cancellation) unless a newer run superseded it.
    pub async fn execute_with_history(
        &self,
        user_prompt: &str,
        prior_history: Vec<ChatMessage>,
    ) -> Result<(String, Vec<ChatMessage>), AgentError> {
        let run = self.begin_run();
        let started = Instant::now();
        tracing::info!(
            target: LIFECYCLE_LOG_TARGET,
            run_id = run.run_id,
            prompt_chars = user_prompt.chars().count(),
            history_messages = prior_history.len(),
            "request started"
        );
        let mut guard = RunGuard {
            run: run.clone(),
            started,
            armed: true,
        };
        run.emit(AgentState::Processing {
            thought_summary: None,
        });

        let outcome = self.run_loop(&run, user_prompt, prior_history).await;
        guard.armed = false;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let canceled = run.is_canceled()
            || matches!(
                outcome,
                Err(RunFailure {
                    error: AgentError::Canceled,
                    ..
                })
            );
        if canceled {
            tracing::info!(
                target: LIFECYCLE_LOG_TARGET,
                run_id = run.run_id,
                elapsed_ms,
                superseded = !run.is_current(),
                "request canceled"
            );
            run.emit_final(AgentState::Idle);
            return Err(AgentError::Canceled);
        }

        match outcome {
            Ok((reply, history)) => {
                tracing::info!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id = run.run_id,
                    elapsed_ms,
                    reply_chars = reply.chars().count(),
                    history_messages = history.len(),
                    "request completed"
                );
                run.emit_final(AgentState::Completed {
                    summary: reply.clone(),
                    new_history: history.clone(),
                });
                Ok((reply, history))
            }
            Err(RunFailure { error, history }) => {
                tracing::error!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id = run.run_id,
                    elapsed_ms,
                    error = %error,
                    "request failed"
                );
                run.emit_final(AgentState::Error {
                    message: error.to_string(),
                    new_history: Some(history),
                });
                Err(error)
            }
        }
    }

    async fn run_loop(
        &self,
        run: &RunHandle,
        user_prompt: &str,
        prior_history: Vec<ChatMessage>,
    ) -> Result<(String, Vec<ChatMessage>), RunFailure> {
        let run_id = run.run_id;

        // history_tail tracks just the conversation turns (no system msg, no tool internals)
        // so the caller can persist and pass them back next time.
        let mut history_tail = prior_history.clone();
        history_tail.push(ChatMessage::user(user_prompt));

        let memory_started = Instant::now();
        tracing::info!(target: LIFECYCLE_LOG_TARGET, run_id, "memory retrieval started");
        let recall = tokio::select! {
            biased;
            _ = run.canceled() => {
                return Err(RunFailure::new(AgentError::Canceled, &history_tail));
            }
            recall = tokio::time::timeout(
                MEMORY_RECALL_TIMEOUT,
                self.memory.recall(user_prompt, None, 5),
            ) => recall,
        };
        let context_items = match recall {
            Ok(Ok(items)) => {
                tracing::info!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    items = items.len(),
                    elapsed_ms = memory_started.elapsed().as_millis() as u64,
                    "memory retrieval completed"
                );
                items
            }
            Ok(Err(error)) => {
                tracing::warn!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    error = %error,
                    "memory retrieval failed; continuing without memory context"
                );
                Vec::new()
            }
            Err(_) => {
                tracing::warn!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    timeout_ms = MEMORY_RECALL_TIMEOUT.as_millis() as u64,
                    "memory retrieval timed out; continuing without memory context"
                );
                Vec::new()
            }
        };

        if run.is_canceled() {
            return Err(RunFailure::new(AgentError::Canceled, &history_tail));
        }

        let mut system_prompt = "\
You are Function, a fast, proactive, native agentic desktop AI assistant.

Operating System Awareness:
- You run directly on the host computer. Detect the OS and interact using native capabilities.
- NEVER assume Windows when running on macOS or Linux, and vice-versa.
- NEVER use Windows commands like `start chrome.exe` or `dir` on macOS or Linux.
- NEVER use raw shell commands to open or control desktop applications. Always use the native desktop tools.

Primary High-Level Desktop Capabilities:
1. `open_app`: Launch or focus applications natively by name (e.g. {\"name\": \"Google Chrome\"}, {\"name\": \"Terminal\"}). Uses native OS APIs on macOS, Windows, and Linux.
2. `close_app`: Gracefully quit or terminate applications by name.
3. `take_screenshot`: Capture the current screen state. Call this whenever you need visual context, after clicking or opening apps to verify the result, or when the user asks what is on screen.
4. `click`: Click at specified (x, y) coordinates or current cursor location.
5. `double_click`: Double click at specified (x, y) coordinates.
6. `type_text`: Type text or unicode characters into the active input field or window.
7. `press_key`: Send key presses (e.g. \"return\", \"space\", \"tab\", \"escape\") or keyboard shortcuts (e.g. \"cmd+t\", \"ctrl+c\", \"alt+f4\").
8. `scroll`: Scroll vertically or horizontally by a given amount.
9. `execute_command`: Run shell commands ONLY when no high-level native tool exists. Safe commands run automatically; destructive commands pause for user confirmation.
10. `fs`: Read, write, or list filesystem directories and files.
11. `web_search`: Search the web for current documentation, news, or knowledge.

Agentic Multi-Step & Observation Loop:
- Plan -> Execute -> Observe -> Recover/Iterate -> Conclude.
- For multi-step tasks (e.g., 'Open Chrome, go to YouTube, and check views'):
  1. Use `open_app` to launch the application.
  2. Take a screenshot with `take_screenshot` to observe and verify the UI state.
  3. Click, type, or press keys to navigate to the target.
  4. Take a screenshot to inspect the new visual context and read the result.
  5. Continue iteratively until the user's objective is fully accomplished.
- If an action or tool fails, do NOT immediately abort or dump raw errors. Inspect the structured error, take a screenshot if visual insight helps, attempt an alternative recovery path, and continue.
- Keep the final response clear, concise, and beautifully formatted in markdown."
            .to_string();

        let memory_items = context_items.len();
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
        let system_prompt_chars = system_prompt.chars().count();
        messages.push(ChatMessage::system(system_prompt));
        messages.extend(prior_history);
        messages.push(ChatMessage::user(user_prompt));

        tracing::info!(
            target: LIFECYCLE_LOG_TARGET,
            run_id,
            system_prompt_chars,
            memory_items,
            tools = tool_definitions.len(),
            messages = messages.len(),
            "context prepared"
        );

        let mut step = 0;
        let mut final_result = String::new();
        let mut consecutive_repeated_tool_calls = 0;
        let mut last_step_tool_signature = String::new();

        while step < self.max_steps {
            if run.is_canceled() {
                return Err(RunFailure::new(AgentError::Canceled, &history_tail));
            }
            step += 1;

            let raw_provider = match self.provider.read() {
                Ok(provider) => provider.clone(),
                Err(e) => {
                    return Err(RunFailure::new(
                        AgentError::Provider(format!("Provider lock poisoned: {e}")),
                        &history_tail,
                    ))
                }
            };

            let retry_run = run.clone();
            let provider: Arc<dyn LlmProvider> =
                Arc::new(RetryingLlmProvider::with_status_callback(
                    raw_provider,
                    Arc::new(move |status_msg| {
                        tracing::warn!(
                            target: LIFECYCLE_LOG_TARGET,
                            run_id,
                            step,
                            status = %status_msg,
                            "provider retry started"
                        );
                        retry_run.emit(AgentState::Processing {
                            thought_summary: Some(status_msg),
                        });
                    }),
                ));

            let limit_setting = *self
                .input_token_limit
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let target_budget = limit_setting.token_budget(provider.context_limit("default"));

            let context_optimization_enabled = *self
                .context_optimization_enabled
                .read()
                .unwrap_or_else(|e| e.into_inner());

            let compaction_started = Instant::now();
            let (request_messages, compaction) = if context_optimization_enabled {
                context::compact_messages(&messages, &tool_definitions, target_budget)
            } else {
                let estimated_tokens = context::estimate_tokens(&messages, &tool_definitions);
                (
                    messages.clone(),
                    context::ContextCompactionReport {
                        before_tokens: estimated_tokens,
                        after_tokens: estimated_tokens,
                        target_budget,
                        ..Default::default()
                    },
                )
            };

            if context_optimization_enabled {
                tracing::info!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    step,
                    provider = %provider.name(),
                    before_tokens = compaction.before_tokens,
                    after_tokens = compaction.after_tokens,
                    budget = target_budget,
                    removed_messages = compaction.removed_messages,
                    removed_images = compaction.removed_images,
                    truncated_outputs = compaction.truncated_outputs,
                    elapsed_ms = compaction_started.elapsed().as_millis() as u64,
                    "context compaction completed"
                );
                if compaction.budget_unreachable {
                    tracing::warn!(
                        target: LIFECYCLE_LOG_TARGET,
                        run_id,
                        step,
                        after_tokens = compaction.after_tokens,
                        budget = target_budget,
                        "context exceeds the input token budget even after compaction; dispatching anyway"
                    );
                }

                if compaction.before_tokens > compaction.after_tokens {
                    let msg = format!(
                        "Context optimized {} → {} tokens",
                        context::format_token_k(compaction.before_tokens),
                        context::format_token_k(compaction.after_tokens)
                    );
                    run.emit(AgentState::Processing {
                        thought_summary: Some(msg),
                    });
                }
            }

            let req = CompletionRequest {
                model: "default".to_string(),
                messages: request_messages,
                tools: tool_definitions.clone(),
                temperature: Some(0.7),
            };

            // Apply configured request delay before dispatch (separate from rate-limit retries)
            let delay_ms = self.request_delay_ms.load(Ordering::Relaxed);
            if delay_ms > 0 {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                    _ = run.canceled() => {
                        return Err(RunFailure::new(AgentError::Canceled, &history_tail));
                    }
                }
            }

            let request_started = Instant::now();
            tracing::info!(
                target: LIFECYCLE_LOG_TARGET,
                run_id,
                step,
                provider = %provider.name(),
                messages = req.messages.len(),
                estimated_tokens = compaction.after_tokens,
                "provider request started"
            );

            let stream_run = run.clone();
            let mut stream_accumulated = String::new();
            let mut first_token_seen = false;
            let on_token = Box::new(move |token: String| {
                if !first_token_seen {
                    first_token_seen = true;
                    tracing::info!(
                        target: LIFECYCLE_LOG_TARGET,
                        run_id,
                        step,
                        first_token_ms = request_started.elapsed().as_millis() as u64,
                        "first stream token received"
                    );
                }
                stream_accumulated.push_str(&token);
                stream_run.emit(AgentState::Streaming {
                    chunk: token,
                    accumulated: stream_accumulated.clone(),
                });
            });

            let response = tokio::select! {
                biased;
                _ = run.canceled() => {
                    tracing::info!(
                        target: LIFECYCLE_LOG_TARGET,
                        run_id,
                        step,
                        elapsed_ms = request_started.elapsed().as_millis() as u64,
                        "provider request abandoned after cancellation"
                    );
                    return Err(RunFailure::new(AgentError::Canceled, &history_tail));
                }
                response = provider.complete_stream(req, on_token) => response,
            };

            let response = match response {
                Ok(res) => res,
                Err(e) => {
                    tracing::error!(
                        target: LIFECYCLE_LOG_TARGET,
                        run_id,
                        step,
                        provider = %provider.name(),
                        elapsed_ms = request_started.elapsed().as_millis() as u64,
                        error = %e,
                        "provider request failed"
                    );
                    return Err(RunFailure::new(
                        AgentError::Provider(e.to_string()),
                        &history_tail,
                    ));
                }
            };

            let response_msg = response.message;
            let tool_call_count = response_msg.tool_calls.as_ref().map_or(0, Vec::len);
            tracing::info!(
                target: LIFECYCLE_LOG_TARGET,
                run_id,
                step,
                provider = %provider.name(),
                elapsed_ms = request_started.elapsed().as_millis() as u64,
                content_chars = response_msg.content.chars().count(),
                tool_calls = tool_call_count,
                finish_reason = ?response.finish_reason,
                "provider request completed"
            );

            if run.is_canceled() {
                return Err(RunFailure::new(AgentError::Canceled, &history_tail));
            }

            if tool_call_count == 0 {
                if response_msg.content.trim().is_empty() {
                    return Err(RunFailure::new(
                        AgentError::Provider(
                            "The model returned an empty response. Please try again.".to_string(),
                        ),
                        &history_tail,
                    ));
                }
                final_result = response_msg.content.clone();
                messages.push(response_msg);
                break;
            }

            let tool_calls = response_msg.tool_calls.clone().unwrap_or_default();

            // Create signature for current tool calls to detect repeated tool-call loops
            let current_sig = tool_calls
                .iter()
                .map(|c| format!("{}:{}", c.name, c.arguments))
                .collect::<Vec<_>>()
                .join("|");

            if !current_sig.is_empty() && current_sig == last_step_tool_signature {
                consecutive_repeated_tool_calls += 1;
            } else {
                consecutive_repeated_tool_calls = 1;
                last_step_tool_signature = current_sig.clone();
            }

            if consecutive_repeated_tool_calls >= 3 {
                let loop_msg = format!(
                    "Identical tool call repeated {} times: {}",
                    consecutive_repeated_tool_calls, tool_calls[0].name
                );
                tracing::warn!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    step,
                    tool = %tool_calls[0].name,
                    repeated_count = consecutive_repeated_tool_calls,
                    "Repeated tool-call loop detected"
                );
                return Err(RunFailure::new(
                    AgentError::RepeatedToolCalls(loop_msg),
                    &history_tail,
                ));
            }

            messages.push(response_msg);

            for call in &tool_calls {
                if run.is_canceled() {
                    return Err(RunFailure::new(AgentError::Canceled, &history_tail));
                }

                run.emit(AgentState::Acting {
                    action_description: describe_tool_call(call),
                });

                let tool_started = Instant::now();
                tracing::info!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    step,
                    tool = %call.name,
                    argument_bytes = call.arguments.to_string().len(),
                    "tool call started"
                );

                let result = match self.run_tool(run, call).await {
                    Some(result) => result,
                    None => {
                        tracing::info!(
                            target: LIFECYCLE_LOG_TARGET,
                            run_id,
                            step,
                            tool = %call.name,
                            elapsed_ms = tool_started.elapsed().as_millis() as u64,
                            "tool call abandoned after cancellation"
                        );
                        return Err(RunFailure::new(AgentError::Canceled, &history_tail));
                    }
                };

                tracing::info!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id,
                    step,
                    tool = %call.name,
                    success = result.success,
                    summary = %result.summary,
                    elapsed_ms = tool_started.elapsed().as_millis() as u64,
                    "tool call completed"
                );

                // Extract screenshot base64 if visual observation was captured
                let mut screenshot_base64 = None;
                if call.name == "take_screenshot" || call.name == "computer_screen" {
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
                        "Visual screen observation:",
                        vec![b64],
                    ));
                }
            }

            run.emit(AgentState::Processing {
                thought_summary: Some("Evaluating progress...".to_string()),
            });
        }

        if final_result.is_empty() {
            tracing::warn!(
                target: LIFECYCLE_LOG_TARGET,
                run_id,
                step_limit = self.max_steps,
                "Agent execution step limit reached"
            );
            return Err(RunFailure::new(
                AgentError::StepLimitExceeded,
                &history_tail,
            ));
        }

        // Append the assistant reply to the history tail for the caller to store
        history_tail.push(ChatMessage::assistant(final_result.clone()));
        Ok((final_result, history_tail))
    }

    /// Run a tool off the async worker threads so blocking tool code (shell
    /// commands, AppleScript, screen capture) cannot stall the runtime, and so
    /// cancellation and the tool timeout can always interrupt the wait.
    /// Returns `None` if the run was canceled while the tool was running.
    async fn run_tool(
        &self,
        run: &RunHandle,
        call: &function_providers::ToolCall,
    ) -> Option<ToolResult> {
        let ctx = ToolContext {
            session_id: "default".to_string(),
            // Function is configured as a fully agentic assistant.
            // Tool implementations still enforce RESTRICTED actions,
            // but confirmation-gated actions are allowed after the
            // user explicitly enabled maximum agentic permissions.
            allow_sensitive: true,
        };
        let tools = self.tools.clone();
        let name = call.name.clone();
        let arguments = call.arguments.clone();
        let runtime = tokio::runtime::Handle::current();
        let task = tokio::task::spawn_blocking(move || {
            runtime.block_on(async move { tools.execute(&name, arguments, &ctx).await })
        });

        let outcome = tokio::select! {
            biased;
            _ = run.canceled() => return None,
            outcome = tokio::time::timeout(TOOL_EXECUTION_TIMEOUT, task) => outcome,
        };

        let result = match outcome {
            Ok(Ok(Ok(res))) => res,
            Ok(Ok(Err(function_tools::ToolError::RequiresConfirmation))) => {
                run.emit(AgentState::WaitingForConfirmation {
                    action: call.name.clone(),
                    details: format!("{}", call.arguments),
                });
                ToolResult::failure(
                    format!("Action '{}' requires user confirmation", call.name),
                    "Action paused: Operation requires explicit user confirmation before executing.",
                )
            }
            Ok(Ok(Err(e))) => {
                ToolResult::failure(format!("Failed to run {}", call.name), e.to_string())
            }
            Ok(Err(join_error)) => {
                tracing::error!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id = run.run_id,
                    tool = %call.name,
                    error = %join_error,
                    "tool call panicked"
                );
                ToolResult::failure(
                    format!("Failed to run {}", call.name),
                    format!("Tool crashed: {join_error}"),
                )
            }
            Err(_) => {
                tracing::warn!(
                    target: LIFECYCLE_LOG_TARGET,
                    run_id = run.run_id,
                    tool = %call.name,
                    timeout_secs = TOOL_EXECUTION_TIMEOUT.as_secs(),
                    "tool call timed out"
                );
                ToolResult::failure(
                    format!("{} timed out", call.name),
                    format!(
                        "Tool did not finish within {} seconds",
                        TOOL_EXECUTION_TIMEOUT.as_secs()
                    ),
                )
            }
        };
        Some(result)
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

    struct RepeatingToolMockProvider;

    #[async_trait::async_trait]
    impl function_providers::LlmProvider for RepeatingToolMockProvider {
        fn name(&self) -> &str {
            "repeating-tool-mock"
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        async fn complete(
            &self,
            _req: function_providers::CompletionRequest,
        ) -> Result<function_providers::CompletionResponse, function_providers::ProviderError>
        {
            Ok(function_providers::CompletionResponse {
                message: ChatMessage {
                    role: function_providers::MessageRole::Assistant,
                    content: String::new(),
                    images: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![function_providers::ToolCall {
                        id: "call_repeat_1".to_string(),
                        name: "execute_command".to_string(),
                        arguments: serde_json::json!({ "command": "echo test" }),
                        thought_signature: None,
                    }]),
                    thought_signature: None,
                },
                finish_reason: Some("tool_calls".to_string()),
            })
        }
    }

    #[tokio::test]
    async fn test_agent_max_steps_configuration_and_preserves_history() {
        let provider = Arc::new(RepeatingToolMockProvider);
        let tools = ToolRegistry::new();
        let memory = Arc::new(InMemoryMemoryStore::new());
        let agent = Agent::new(provider, tools, memory).with_max_steps(2);

        let mut rx = agent.subscribe_state();
        let result = agent.execute_task("Run infinite loop").await;

        assert!(matches!(result, Err(AgentError::StepLimitExceeded)));

        let mut found_error_with_history = false;
        while let Ok(state) = rx.try_recv() {
            if let AgentState::Error { message, new_history } = state {
                assert!(message.contains("Execution step limit exceeded"));
                assert!(new_history.is_some());
                let hist = new_history.unwrap();
                assert!(!hist.is_empty());
                found_error_with_history = true;
            }
        }
        assert!(found_error_with_history);
    }

    #[tokio::test]
    async fn test_agent_detects_repeated_tool_call_loop() {
        let provider = Arc::new(RepeatingToolMockProvider);
        let tools = ToolRegistry::new();
        let memory = Arc::new(InMemoryMemoryStore::new());
        let agent = Agent::new(provider, tools, memory).with_max_steps(10);

        let mut rx = agent.subscribe_state();
        let result = agent.execute_task("Do repeating tool call").await;

        assert!(matches!(result, Err(AgentError::RepeatedToolCalls(_))));

        let mut found_error_with_history = false;
        while let Ok(state) = rx.try_recv() {
            if let AgentState::Error { message, new_history } = state {
                assert!(message.contains("Repeated tool call loop detected"));
                assert!(new_history.is_some());
                found_error_with_history = true;
            }
        }
        assert!(found_error_with_history);
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

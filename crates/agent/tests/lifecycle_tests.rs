//! Regression tests for the "stuck on Thinking..." hang: every request must
//! reach a terminal state (Completed, Error, or Idle on cancel) and every
//! subscriber must be able to observe it.

use async_trait::async_trait;
use function_agent::context::compact_messages;
use function_agent::{Agent, AgentError, AgentState};
use function_memory::{InMemoryMemoryStore, MemoryCategory, MemoryError, MemoryItem, MemoryStore};
use function_providers::{
    ChatMessage, CompletionRequest, CompletionResponse, LlmProvider, MockLlmProvider,
    ProviderError, ToolDefinition,
};
use function_tools::ToolRegistry;
use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::{self, error::RecvError};

const TEST_DEADLINE: Duration = Duration::from_secs(10);

/// Provider whose first `hang_calls` streams never resolve (simulating a
/// provider connection that stays open without sending data); later calls
/// succeed.
struct HangingProvider {
    calls: AtomicUsize,
    hang_calls: usize,
}

impl HangingProvider {
    fn new(hang_calls: usize) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            hang_calls,
        }
    }
}

#[async_trait]
impl LlmProvider for HangingProvider {
    fn name(&self) -> &str {
        "hanging"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) < self.hang_calls {
            std::future::pending::<()>().await;
        }
        Ok(CompletionResponse {
            message: ChatMessage::assistant("recovered"),
            finish_reason: Some("stop".to_string()),
        })
    }
}

/// Provider that streams many tokens synchronously, faster than any UI
/// subscriber can drain the state channel.
struct BurstProvider {
    tokens: usize,
}

#[async_trait]
impl LlmProvider for BurstProvider {
    fn name(&self) -> &str {
        "burst"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        unreachable!("only streaming is used")
    }

    async fn complete_stream(
        &self,
        _req: CompletionRequest,
        mut on_token: Box<dyn FnMut(String) + Send>,
    ) -> Result<CompletionResponse, ProviderError> {
        let mut content = String::new();
        for i in 0..self.tokens {
            let token = format!("t{i} ");
            content.push_str(&token);
            on_token(token);
        }
        Ok(CompletionResponse {
            message: ChatMessage::assistant(content),
            finish_reason: Some("stop".to_string()),
        })
    }
}

struct PanickingProvider;

#[async_trait]
impl LlmProvider for PanickingProvider {
    fn name(&self) -> &str {
        "panicking"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        panic!("provider bug");
    }
}

struct FailingMemory;

#[async_trait]
impl MemoryStore for FailingMemory {
    async fn remember(&self, _item: MemoryItem) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn recall(
        &self,
        _query: &str,
        _category: Option<MemoryCategory>,
        _limit: usize,
    ) -> Result<Vec<MemoryItem>, MemoryError> {
        Err(MemoryError::Io("disk unavailable".to_string()))
    }

    async fn forget(&self, _id: &str) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn clear_category(&self, _category: MemoryCategory) -> Result<(), MemoryError> {
        Ok(())
    }
}

fn agent_with(provider: Arc<dyn LlmProvider>) -> Arc<Agent> {
    Arc::new(Agent::new(
        provider,
        ToolRegistry::new(),
        Arc::new(InMemoryMemoryStore::new()),
    ))
}

fn is_terminal(state: &AgentState) -> bool {
    matches!(
        state,
        AgentState::Idle | AgentState::Completed { .. } | AgentState::Error { .. }
    )
}

/// Mirrors the UI subscription loop: lagging must skip ahead, not stop.
async fn next_terminal_state(rx: &mut broadcast::Receiver<AgentState>) -> AgentState {
    tokio::time::timeout(TEST_DEADLINE, async {
        loop {
            match rx.recv().await {
                Ok(state) if is_terminal(&state) => return state,
                Ok(_) | Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => panic!("state channel closed before terminal state"),
            }
        }
    })
    .await
    .expect("no terminal state was broadcast")
}

#[test]
fn compaction_terminates_when_budget_is_unreachable() {
    // A system prompt larger than the whole budget used to make the final
    // truncation pass re-select an already-truncated message forever,
    // freezing the request before the provider was ever called.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let messages = vec![
            ChatMessage::system("s".repeat(20_000)),
            ChatMessage::user("u".repeat(5_000)),
            ChatMessage::assistant("a".repeat(5_000)),
            ChatMessage::user("current question"),
        ];
        let tools: Vec<ToolDefinition> = vec![];
        let _ = tx.send(compact_messages(&messages, &tools, 2_000));
    });
    let (compacted, report) = rx
        .recv_timeout(TEST_DEADLINE)
        .expect("context compaction did not terminate");

    assert!(report.budget_unreachable);
    assert_eq!(
        compacted[0].content.len(),
        20_000,
        "system prompt is preserved"
    );
    assert_eq!(
        compacted.last().unwrap().content,
        "current question",
        "latest user turn is preserved"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_interrupts_hung_provider_and_returns_to_idle() {
    let agent = agent_with(Arc::new(HangingProvider::new(usize::MAX)));
    let mut rx = agent.subscribe_state();

    let run = tokio::spawn({
        let agent = agent.clone();
        async move { agent.execute_task("hello").await }
    });

    // Wait until the run is in flight, then cancel.
    let first = tokio::time::timeout(TEST_DEADLINE, rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, AgentState::Processing { .. }));
    tokio::time::sleep(Duration::from_millis(50)).await;
    agent.cancel();

    let result = tokio::time::timeout(TEST_DEADLINE, run)
        .await
        .expect("canceled run never resolved")
        .unwrap();
    assert!(matches!(result, Err(AgentError::Canceled)));
    assert!(matches!(
        next_terminal_state(&mut rx).await,
        AgentState::Idle
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn new_request_supersedes_hung_request() {
    let agent = agent_with(Arc::new(HangingProvider::new(1)));
    let mut rx = agent.subscribe_state();

    let stuck = tokio::spawn({
        let agent = agent.clone();
        async move { agent.execute_task("first").await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let second = tokio::time::timeout(TEST_DEADLINE, agent.execute_task("second"))
        .await
        .expect("second request hung");
    assert_eq!(second.unwrap(), "recovered");

    let first = tokio::time::timeout(TEST_DEADLINE, stuck)
        .await
        .expect("superseded request never resolved")
        .unwrap();
    assert!(matches!(first, Err(AgentError::Canceled)));

    // The superseded run must not broadcast a stale Idle over the new result.
    let terminal = next_terminal_state(&mut rx).await;
    assert!(
        matches!(terminal, AgentState::Completed { .. }),
        "got {terminal:?}"
    );
    assert!(!agent.is_canceled());
}

#[tokio::test]
async fn lagging_subscriber_still_observes_completion() {
    // More streamed tokens than the state channel can buffer; a subscriber
    // that is not polled during the burst must still see the final state.
    let agent = agent_with(Arc::new(BurstProvider { tokens: 2_000 }));
    let mut rx = agent.subscribe_state();

    agent.execute_task("stream a lot").await.unwrap();

    assert!(matches!(rx.recv().await, Err(RecvError::Lagged(_))));
    let terminal = next_terminal_state(&mut rx).await;
    assert!(
        matches!(terminal, AgentState::Completed { .. }),
        "got {terminal:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_panic_still_broadcasts_error_state() {
    let agent = agent_with(Arc::new(PanickingProvider));
    let mut rx = agent.subscribe_state();

    let run = tokio::spawn({
        let agent = agent.clone();
        async move { agent.execute_task("hello").await }
    });
    assert!(run.await.unwrap_err().is_panic());
    assert!(matches!(
        next_terminal_state(&mut rx).await,
        AgentState::Error { .. }
    ));
}

#[tokio::test]
async fn memory_failure_does_not_block_the_request() {
    let agent = Agent::new(
        Arc::new(MockLlmProvider::new("Done.")),
        ToolRegistry::new(),
        Arc::new(FailingMemory),
    );
    let mut rx = agent.subscribe_state();

    assert_eq!(agent.execute_task("hello").await.unwrap(), "Done.");
    assert!(matches!(
        next_terminal_state(&mut rx).await,
        AgentState::Completed { .. }
    ));
}

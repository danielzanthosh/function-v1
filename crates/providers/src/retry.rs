use crate::{CompletionRequest, CompletionResponse, LlmProvider, ProviderError};
use async_trait::async_trait;
use rand::Rng;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

pub type StatusCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Provider wrapper that automatically retries rate-limited LLM requests.
pub struct RetryingLlmProvider {
    inner: Arc<dyn LlmProvider>,
    max_retries: usize,
    status_callback: Option<StatusCallback>,
}

impl RetryingLlmProvider {
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        let inner = if let Some(existing) = inner.as_any().downcast_ref::<RetryingLlmProvider>() {
            existing.inner.clone()
        } else {
            inner
        };
        Self {
            inner,
            max_retries: 10,
            status_callback: None,
        }
    }

    pub fn with_status_callback(
        inner: Arc<dyn LlmProvider>,
        status_callback: StatusCallback,
    ) -> Self {
        let mut provider = Self::new(inner);
        provider.status_callback = Some(status_callback);
        provider
    }

    pub fn inner(&self) -> &Arc<dyn LlmProvider> {
        &self.inner
    }

    fn calculate_backoff(&self, attempt: usize, provider_delay: Option<Duration>) -> Duration {
        if let Some(delay) = provider_delay {
            return delay;
        }

        // Exponential backoff: 500ms * 2^(attempt - 1), capped at 30s
        let base_ms = 500u64.saturating_mul(1u64 << (attempt.saturating_sub(1).min(6)));
        let capped_ms = base_ms.min(30_000);

        // Add small jitter (0..250ms)
        let jitter = rand::thread_rng().gen_range(0..=250);
        Duration::from_millis(capped_ms + jitter)
    }

    fn notify_status(&self, attempt: usize, max_attempts: usize) {
        let message = format!("Rate limited · Retrying {}/{}…", attempt + 1, max_attempts);
        if let Some(ref cb) = self.status_callback {
            cb(message);
        }
    }
}

#[async_trait]
impl LlmProvider for RetryingLlmProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn context_limit(&self, model: &str) -> usize {
        self.inner.context_limit(model)
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let mut attempt = 1;
        loop {
            match self.inner.complete(req.clone()).await {
                Ok(res) => return Ok(res),
                Err(err) => {
                    if err.is_permanent() || !err.is_rate_limit() || attempt >= self.max_retries {
                        return Err(err);
                    }

                    let delay = self.calculate_backoff(attempt, err.retry_after());
                    tracing::warn!(
                        provider = %self.inner.name(),
                        attempt = attempt + 1,
                        max_attempts = self.max_retries,
                        delay_ms = delay.as_millis(),
                        error = %err,
                        "Rate limit encountered on request dispatch; initiating retry"
                    );

                    self.notify_status(attempt, self.max_retries);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
        mut on_token: Box<dyn FnMut(String) + Send>,
    ) -> Result<CompletionResponse, ProviderError> {
        let mut attempt = 1;
        loop {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let stream_cb = Box::new(move |token: String| {
                let _ = tx.send(token);
            });

            let res = self.inner.complete_stream(req.clone(), stream_cb).await;

            let mut attempt_tokens = Vec::new();
            while let Ok(token) = rx.try_recv() {
                attempt_tokens.push(token);
            }

            match res {
                Ok(response) => {
                    for token in attempt_tokens {
                        on_token(token);
                    }
                    return Ok(response);
                }
                Err(err) => {
                    if err.is_permanent() || !err.is_rate_limit() || attempt >= self.max_retries {
                        return Err(err);
                    }

                    let delay = self.calculate_backoff(attempt, err.retry_after());
                    tracing::warn!(
                        provider = %self.inner.name(),
                        attempt = attempt + 1,
                        max_attempts = self.max_retries,
                        delay_ms = delay.as_millis(),
                        error = %err,
                        "Rate limit encountered on streaming request; initiating retry"
                    );

                    self.notify_status(attempt, self.max_retries);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatMessage;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FailingThenSucceedingProvider {
        fail_count: usize,
        attempts: AtomicUsize,
    }

    #[async_trait]
    impl LlmProvider for FailingThenSucceedingProvider {
        fn name(&self) -> &str {
            "failing-test-provider"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            let current = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
            if current <= self.fail_count {
                Err(ProviderError::RateLimit {
                    code: 429,
                    message: "Rate limit reached. Please try again in 100ms.".to_string(),
                    retry_after: Some(Duration::from_millis(10)),
                })
            } else {
                Ok(CompletionResponse {
                    message: ChatMessage::assistant("Success after retries"),
                    finish_reason: Some("stop".to_string()),
                })
            }
        }
    }

    #[tokio::test]
    async fn test_retrying_provider_succeeds_after_rate_limits() {
        let inner = Arc::new(FailingThenSucceedingProvider {
            fail_count: 2,
            attempts: AtomicUsize::new(0),
        });

        let status_updates = Arc::new(std::sync::Mutex::new(Vec::new()));
        let status_updates_clone = status_updates.clone();

        let retrying = RetryingLlmProvider::with_status_callback(
            inner,
            Arc::new(move |msg| {
                status_updates_clone.lock().unwrap().push(msg);
            }),
        );

        let req = CompletionRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage::user("hello")],
            tools: vec![],
            temperature: None,
        };

        let res = retrying.complete(req).await.unwrap();
        assert_eq!(res.message.content, "Success after retries");

        let updates = status_updates.lock().unwrap();
        assert_eq!(updates.len(), 2);
        assert!(updates[0].contains("Rate limited · Retrying 2/10…"));
        assert!(updates[1].contains("Rate limited · Retrying 3/10…"));
    }

    struct PermanentErrorProvider;

    #[async_trait]
    impl LlmProvider for PermanentErrorProvider {
        fn name(&self) -> &str {
            "permanent-error-provider"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            Err(ProviderError::Api {
                code: 401,
                message: "Invalid API Key provided".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_retrying_provider_does_not_retry_permanent_error() {
        let inner = Arc::new(PermanentErrorProvider);
        let retrying = RetryingLlmProvider::new(inner);

        let req = CompletionRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage::user("hello")],
            tools: vec![],
            temperature: None,
        };

        let err = retrying.complete(req).await.unwrap_err();
        assert!(err.is_permanent());
    }

    #[test]
    fn test_nested_retrying_provider_unwraps_inner() {
        let base = Arc::new(PermanentErrorProvider);
        let retrying1 = Arc::new(RetryingLlmProvider::new(base));
        let retrying2 = RetryingLlmProvider::new(retrying1.clone());
        assert_eq!(retrying2.name(), "permanent-error-provider");
    }
}

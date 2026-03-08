use std::sync::atomic::{AtomicUsize, Ordering};

use alloy_primitives::{B256, Bytes};
use async_trait::async_trait;
use tracing::{debug, warn};

use crate::sender::{CancelOutcome, SubmissionContext, SubmissionStrategy, SubmitError};

/// Strategy router that tries strategies in order and fails over on errors
pub struct StrategyRouter {
    strategies: Vec<Box<dyn SubmissionStrategy>>,
    /// Index of the current primary strategy
    primary: AtomicUsize,
}

impl StrategyRouter {
    pub fn new(strategies: Vec<Box<dyn SubmissionStrategy>>) -> Self {
        Self {
            strategies,
            primary: AtomicUsize::new(0),
        }
    }

    pub fn reset_primary(&self) {
        self.primary.store(0, Ordering::Relaxed);
    }
}

#[async_trait]
impl SubmissionStrategy for StrategyRouter {
    fn name(&self) -> &str {
        "router"
    }

    async fn submit(&self, raw_tx: Bytes, ctx: &SubmissionContext) -> Result<B256, SubmitError> {
        let primary_index = self.primary.load(Ordering::Relaxed);
        let n = self.strategies.len();

        for offset in 0..n {
            let idx = (primary_index + offset) % n;
            let strat = &self.strategies[idx];

            match strat.submit(raw_tx.clone(), ctx).await {
                Ok(hash) => {
                    if offset > 0 {
                        self.primary.store(idx, Ordering::Relaxed);
                        debug!(
                            new_primary = strat.name(),
                            "failed over to new primary strategy"
                        );
                    }
                    return Ok(hash);
                }
                Err(SubmitError::Retryable(msg)) => {
                    // Retryable errors don't trigger failover
                    return Err(SubmitError::Retryable(msg));
                }
                Err(SubmitError::Terminal(msg)) => {
                    // Terminal errors stop all attempts
                    return Err(SubmitError::Terminal(msg));
                }
                Err(SubmitError::StrategyFailed(msg)) => {
                    warn!(
                        strategy = strat.name(),
                        error = msg,
                        "strategy failed, trying next"
                    );
                    continue;
                }
            }
        }

        Err(SubmitError::StrategyFailed(
            "all strategies exhausted".into(),
        ))
    }

    async fn cancel(
        &self,
        tx_hash: B256,
        cancel_tx: Option<Bytes>,
    ) -> Result<CancelOutcome, SubmitError> {
        let primary_idx = self.primary.load(Ordering::Relaxed);
        self.strategies[primary_idx]
            .cancel(tx_hash, cancel_tx)
            .await
    }

    fn supports_soft_cancel(&self) -> bool {
        let primary_idx = self.primary.load(Ordering::Relaxed);
        self.strategies[primary_idx].supports_soft_cancel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test strategy that always succeeds
    struct SuccessStrategy;

    #[async_trait]
    impl SubmissionStrategy for SuccessStrategy {
        fn name(&self) -> &str {
            "success"
        }

        async fn submit(
            &self,
            _raw_tx: Bytes,
            _ctx: &SubmissionContext,
        ) -> Result<B256, SubmitError> {
            Ok(B256::repeat_byte(0x01))
        }

        async fn cancel(
            &self,
            _tx_hash: B256,
            _cancel_tx: Option<Bytes>,
        ) -> Result<CancelOutcome, SubmitError> {
            Ok(CancelOutcome::Cancelled)
        }

        fn supports_soft_cancel(&self) -> bool {
            false
        }
    }

    /// A test strategy that always fails
    struct FailStrategy;

    #[async_trait]
    impl SubmissionStrategy for FailStrategy {
        fn name(&self) -> &str {
            "fail"
        }

        async fn submit(
            &self,
            _raw_tx: Bytes,
            _ctx: &SubmissionContext,
        ) -> Result<B256, SubmitError> {
            Err(SubmitError::StrategyFailed("always fails".into()))
        }

        async fn cancel(
            &self,
            _tx_hash: B256,
            _cancel_tx: Option<Bytes>,
        ) -> Result<CancelOutcome, SubmitError> {
            Err(SubmitError::StrategyFailed("always fails".into()))
        }

        fn supports_soft_cancel(&self) -> bool {
            false
        }
    }

    fn test_ctx() -> SubmissionContext {
        SubmissionContext {
            expected_storage: None,
            chain_id: 1,
        }
    }

    #[tokio::test]
    async fn test_primary_succeeds() {
        let router = StrategyRouter::new(vec![Box::new(SuccessStrategy), Box::new(FailStrategy)]);
        let result = router.submit(Bytes::new(), &test_ctx()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_failover_to_second() {
        let router = StrategyRouter::new(vec![Box::new(FailStrategy), Box::new(SuccessStrategy)]);
        let result = router.submit(Bytes::new(), &test_ctx()).await;
        assert!(result.is_ok());
        // Primary should now be updated to index 1
        assert_eq!(router.primary.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_all_strategies_fail() {
        let router = StrategyRouter::new(vec![Box::new(FailStrategy), Box::new(FailStrategy)]);
        let result = router.submit(Bytes::new(), &test_ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reset_primary() {
        let router = StrategyRouter::new(vec![Box::new(FailStrategy), Box::new(SuccessStrategy)]);
        // Trigger failover
        router.submit(Bytes::new(), &test_ctx()).await.unwrap();
        assert_eq!(router.primary.load(Ordering::Relaxed), 1);

        // Reset
        router.reset_primary();
        assert_eq!(router.primary.load(Ordering::Relaxed), 0);
    }
}

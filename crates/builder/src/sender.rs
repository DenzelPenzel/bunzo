use alloy_primitives::{B256, Bytes};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct SubmissionContext {
    pub expected_storage: Option<bunzo_types::validation::ExpectedStorage>,
    pub chain_id: u64,
}

#[derive(Debug, Clone)]
pub enum CancelOutcome {
    Cancelled,
    AlreadyMined,
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    /// Retryable error (try same strategy again)
    #[error("retryable: {0}")]
    Retryable(String),
    /// Strategy-specific failure (failover to next strategy)
    #[error("strategy failed: {0}")]
    StrategyFailed(String),
    /// Terminal error (do not retry)
    #[error("terminal: {0}")]
    Terminal(String),
}

#[async_trait]
pub trait SubmissionStrategy: Send + Sync {
    fn name(&self) -> &str;

    async fn submit(&self, raw_tx: Bytes, ctx: &SubmissionContext) -> Result<B256, SubmitError>;

    async fn cancel(
        &self,
        tx_hash: B256,
        cancel_tx: Option<Bytes>,
    ) -> Result<CancelOutcome, SubmitError>;

    fn supports_soft_cancel(&self) -> bool;
}

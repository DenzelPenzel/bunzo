use std::sync::Arc;

use alloy_primitives::{B256, Bytes};
use async_trait::async_trait;
use tracing::{debug, warn};

use bunzo_provider::traits::EvmProvider;

use crate::sender::{CancelOutcome, SubmissionContext, SubmissionStrategy, SubmitError};

pub struct ConditionalSubmissionStrategy<P> {
    provider: Arc<P>,
    max_slots: usize,
}

impl<P> ConditionalSubmissionStrategy<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self {
            provider,
            max_slots: 1000,
        }
    }

    pub fn with_max_slots(mut self, max_slots: usize) -> Self {
        self.max_slots = max_slots;
        self
    }
}

#[async_trait]
impl<P> SubmissionStrategy for ConditionalSubmissionStrategy<P>
where
    P: EvmProvider,
{
    fn name(&self) -> &str {
        "conditional"
    }

    async fn submit(&self, raw_tx: Bytes, ctx: &SubmissionContext) -> Result<B256, SubmitError> {
        todo!()
    }

    async fn cancel(
        &self,
        tx_hash: B256,
        cancel_tx: Option<Bytes>,
    ) -> Result<CancelOutcome, SubmitError> {
        todo!()
    }

    fn supports_soft_cancel(&self) -> bool {
        false
    }
}

impl<P> ConditionalSubmissionStrategy<P>
where
    P: EvmProvider,
{
    async fn submit_direct(&self, raw_tx: Bytes) -> Result<B256, SubmitError> {
        self.provider
            .send_raw_transaction(raw_tx)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("nonce too low")
                    || msg.contains("already known")
                    || msg.contains("replacement transaction underpriced")
                {
                    SubmitError::Retryable(msg)
                } else if msg.contains("conditional") {
                    // Conditional check failed — the storage state changed
                    SubmitError::Retryable(format!("conditional check failed: {msg}"))
                } else {
                    SubmitError::StrategyFailed(msg)
                }
            })
    }
}

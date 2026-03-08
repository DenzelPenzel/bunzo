use std::sync::Arc;

use alloy_primitives::{B256, Bytes};
use async_trait::async_trait;

use bunzo_provider::traits::EvmProvider;

use crate::sender::{CancelOutcome, SubmissionContext, SubmissionStrategy, SubmitError};

pub struct DirectSubmissionStrategy<P> {
    provider: Arc<P>,
}

impl<P> DirectSubmissionStrategy<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P> SubmissionStrategy for DirectSubmissionStrategy<P>
where
    P: EvmProvider,
{
    fn name(&self) -> &str {
        "direct"
    }

    async fn submit(&self, raw_tx: Bytes, _ctx: &SubmissionContext) -> Result<B256, SubmitError> {
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
                } else {
                    SubmitError::StrategyFailed(msg)
                }
            })
    }

    async fn cancel(
        &self,
        _tx_hash: B256,
        cancel_tx: Option<Bytes>,
    ) -> Result<CancelOutcome, SubmitError> {
        match cancel_tx {
            Some(raw_cancel_tx) => match self.provider.send_raw_transaction(raw_cancel_tx).await {
                Ok(_) => Ok(CancelOutcome::Cancelled),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("nonce too low") || msg.contains("already known") {
                        Ok(CancelOutcome::AlreadyMined)
                    } else {
                        Err(SubmitError::StrategyFailed(msg))
                    }
                }
            },
            None => Ok(CancelOutcome::Failed(
                "no cancel transaction provided".into(),
            )),
        }
    }

    fn supports_soft_cancel(&self) -> bool {
        false
    }
}

use std::sync::Arc;

use alloy_primitives::Address;
use async_trait::async_trait;
use tracing::{debug, info, warn};

use bunzo_pool::OperationPool;
use bunzo_provider::gas_oracle::GasOracle;
use bunzo_provider::traits::{BundleHandler, HandleOpsOut};
use bunzo_types::chain::ChainSpec;
use bunzo_types::user_operation::UserOperation as UserOperationTrait;

use crate::bundle::{Bundle, RejectedOp};

/// Errors from the bundle proposer
#[derive(Debug, thiserror::Error)]
pub enum ProposerError {
    #[error("provider error: {0}")]
    Provider(#[from] bunzo_provider::ProviderError),

    #[error("no operations available")]
    NoOperations,

    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait BundleProposer: Send + Sync {
    async fn make_bundle(&self) -> Result<Bundle, ProposerError>;
}

pub struct BundleProposerImpl<H, G> {
    pool: Arc<OperationPool>,
    bundle_handler: H,
    gas_oracle: G,
    chain_spec: ChainSpec,
    beneficiary: Address,
    max_retries: usize,
}

impl<H, G> BundleProposerImpl<H, G> {
    pub fn new(
        pool: Arc<OperationPool>,
        bundle_handler: H,
        gas_oracle: G,
        chain_spec: ChainSpec,
        beneficiary: Address,
    ) -> Self {
        Self {
            pool,
            bundle_handler,
            gas_oracle,
            chain_spec,
            beneficiary,
            max_retries: 3,
        }
    }
}

#[async_trait]
impl<H, G> BundleProposer for BundleProposerImpl<H, G>
where
    H: BundleHandler,
    G: GasOracle,
{
    async fn make_bundle(&self) -> Result<Bundle, ProposerError> {
        let current_fees = self.gas_oracle.current_fees().await?;
        let base_fee = self.gas_oracle.base_fee().await?;

        let candidates = self.pool.best_operations(self.chain_spec.max_bundle_size);
        if candidates.is_empty() {
            return Err(ProposerError::NoOperations);
        }

        debug!(
            candidates = candidates.len(),
            base_fee, "starting bundle proposal"
        );

        let min_priority = self.chain_spec.min_max_priority_fee_per_gas;
        let mut ops_with_hashes: Vec<_> = candidates
            .into_iter()
            .filter(|e| {
                let uo_fees = e.uo.gas_fees();
                if !uo_fees.covers(base_fee, min_priority) {
                    debug!(
                        hash = %e.hash,
                        max_fee = uo_fees.max_fee_per_gas,
                        base_fee,
                        "skipping op: gas too low for current conditions"
                    );
                    return false;
                }
                true
            })
            .collect();

        if ops_with_hashes.is_empty() {
            return Err(ProposerError::NoOperations);
        }

        let mut rejected_ops = Vec::new();

        for attempt in 0..=self.max_retries {
            let packed_ops: Vec<_> = ops_with_hashes.iter().map(|e| e.uo.pack()).collect();

            if packed_ops.is_empty() {
                return Err(ProposerError::NoOperations);
            }

            // Estimate gas: sum individual gas limits + per-op overhead + intrinsic gas
            let total_gas = self.estimate_bundle_gas(&ops_with_hashes);

            let res = self
                .bundle_handler
                .call_handle_ops(
                    packed_ops.clone(),
                    self.beneficiary,
                    total_gas,
                    current_fees,
                )
                .await?;

            match res {
                HandleOpsOut::Success => {
                    info!(
                        ops = ops_with_hashes.len(),
                        gas_estimate = total_gas,
                        attempt,
                        "bundle proposal succeeded"
                    );

                    let gas_with_buffer = total_gas + total_gas / 20;

                    return Ok(Bundle {
                        ops: packed_ops,
                        beneficiary: self.beneficiary,
                        gas_estimate: gas_with_buffer,
                        gas_fees: current_fees,
                        rejected_ops,
                    });
                }
                HandleOpsOut::FailedOp(idx, reason) => {
                    if attempt >= self.max_retries {
                        break;
                    }

                    if idx < ops_with_hashes.len() {
                        let removed = ops_with_hashes.remove(idx);
                        warn!(
                            hash = %removed.hash,
                            idx,
                            reason,
                            attempt,
                            "removing failed op from bundle"
                        );
                        rejected_ops.push(RejectedOp {
                            hash: removed.hash,
                            reason,
                        });
                    } else {
                        warn!(
                            idx,
                            ops_count = ops_with_hashes.len(),
                            "FailedOp index out of bounds"
                        );
                        break;
                    }
                }
                HandleOpsOut::SignatureValidationFailed(addr) => {
                    return Err(ProposerError::Other(format!(
                        "aggregator signature validation failed: {addr}"
                    )));
                }
                HandleOpsOut::Revert(data) => {
                    return Err(ProposerError::Other(format!(
                        "handleOps reverted: 0x{}",
                        const_hex::encode(&data)
                    )));
                }
            }
        }

        let packed_ops = ops_with_hashes
            .iter()
            .map(|e| e.uo.pack())
            .collect::<Vec<_>>();

        if packed_ops.is_empty() {
            return Err(ProposerError::NoOperations);
        }

        let total_gas = self.estimate_bundle_gas(&ops_with_hashes);
        let gas_with_buffer = total_gas + total_gas / 20;

        Ok(Bundle {
            ops: packed_ops,
            beneficiary: self.beneficiary,
            gas_estimate: gas_with_buffer,
            gas_fees: current_fees,
            rejected_ops,
        })
    }
}

impl<H, G> BundleProposerImpl<H, G> {
    fn estimate_bundle_gas(&self, ops: &[Arc<bunzo_pool::PoolEntry>]) -> u64 {
        let per_op_gas = self.chain_spec.per_user_op_v0_7_gas;
        let intrinsic = self.chain_spec.transaction_intrinsic_gas;

        let ops_gas: u64 = ops
            .iter()
            .map(|entry| entry.uo.total_gas_limit() as u64)
            .sum();

        intrinsic + ops_gas + (per_op_gas * ops.len() as u64)
    }
}

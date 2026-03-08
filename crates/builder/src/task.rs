use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::Bytes;
use alloy_sol_types::SolCall;
use metrics::{counter, histogram};
use tracing::{debug, error, info, warn};

use bunzo_pool::OperationPool;
use bunzo_provider::traits::EvmProvider;
use bunzo_signer::BundlerSigner;
use bunzo_types::chain::ChainSpec;
use bunzo_types::user_operation::UserOperation as UserOperationTrait;
use bunzo_types::user_operation::v0_7::UserOperation;

use crate::BundleSenderState;
use crate::bundle::Bundle;
use crate::proposer::{BundleProposer, ProposerError};
use crate::sender::{SubmissionContext, SubmissionStrategy};
use crate::tracker::{EscalationConfig, TransactionStatus, TransactionTracker};

pub struct BundlerTask<P, S, ST, EP> {
    proposer: P,
    signer: Arc<S>,
    strategy: ST,
    pool: Arc<OperationPool>,
    provider: Arc<EP>,
    chain_spec: ChainSpec,
    /// Transaction tracker for nonce management and fee escalation
    tracker: TransactionTracker<EP>,
    /// Current FSM state.
    state: BundleSenderState,
    /// The bundle currently being processed (set during Building → Submitting)
    current_bundle: Option<Bundle>,
    /// Interval between bundle attempts when idle
    idle_interval: Duration,
    /// Interval for polling pending transactions
    pending_poll_interval: Duration,
    /// Number of confirmations required before declaring success
    confirmation_depth: u64,
    /// Block number at which the current transaction was mined
    mined_block: Option<u64>,
}

impl<P, S, ST, EP> BundlerTask<P, S, ST, EP>
where
    P: BundleProposer,
    S: BundlerSigner,
    ST: SubmissionStrategy,
    EP: EvmProvider,
{
    pub fn new(
        proposer: P,
        signer: Arc<S>,
        strategy: ST,
        pool: Arc<OperationPool>,
        provider: Arc<EP>,
        chain_spec: ChainSpec,
    ) -> Self {
        let idle_interval = Duration::from_millis(chain_spec.bundle_max_send_interval_millis);
        let signer_addr = signer.address();
        let tracker =
            TransactionTracker::new(provider.clone(), signer_addr, EscalationConfig::default());
        Self {
            proposer,
            signer,
            strategy,
            pool,
            provider,
            chain_spec,
            tracker,
            state: BundleSenderState::Idle,
            current_bundle: None,
            idle_interval,
            pending_poll_interval: Duration::from_secs(2),
            confirmation_depth: 3,
            mined_block: None,
        }
    }

    /// Run the bundling loop
    pub async fn run(mut self, mut shutdown: tokio::sync::watch::Receiver<()>) {
        if let Err(e) = self.tracker.initialize().await {
            error!(error = %e, "failed to initialize transaction tracker");
            return;
        }

        info!(
            interval_ms = self.idle_interval.as_millis(),
            beneficiary = %self.signer.address(),
            "bundler task started (FSM mode)"
        );

        loop {
            let tick = match self.state {
                BundleSenderState::Idle => self.idle_interval,
                BundleSenderState::Pending | BundleSenderState::Confirming => {
                    self.pending_poll_interval
                }
                _ => Duration::from_millis(10),
            };

            tokio::select! {
                _ = tokio::time::sleep(tick) => {
                    self.step().await;
                }
                _ = shutdown.changed() => {
                    info!("bundler task received shutdown signal");
                    break;
                }
            }
        }
    }

    async fn step(&mut self) {
        let prev_state = self.state;
        let next_state = match self.state {
            BundleSenderState::Idle => self.handle_idle().await,
            BundleSenderState::Building => self.handle_building().await,
            BundleSenderState::Submitting => self.handle_submitting().await,
            BundleSenderState::Pending => self.handle_pending().await,
            BundleSenderState::Escalating => self.handle_escalating().await,
            BundleSenderState::Cancelling => self.handle_cancelling().await,
            BundleSenderState::Confirming => self.handle_confirming().await,
            BundleSenderState::Confirmed | BundleSenderState::Abandoned => {
                // Terminal states reset to Idle
                self.current_bundle = None;
                self.mined_block = None;
                BundleSenderState::Idle
            }
        };

        if prev_state != next_state {
            debug!(
                from = ?prev_state,
                to = ?next_state,
                "state transition"
            );
        }
        self.state = next_state;
    }

    /// Idle: check if the pool has enough operations to justify building
    async fn handle_idle(&mut self) -> BundleSenderState {
        if self.pool.size() == 0 {
            return BundleSenderState::Idle;
        }
        BundleSenderState::Building
    }

    /// Building: propose a bundle from the pool
    async fn handle_building(&mut self) -> BundleSenderState {
        let bundle = match self.proposer.make_bundle().await {
            Ok(bundle) => bundle,
            Err(ProposerError::NoOperations) => return BundleSenderState::Idle,
            Err(e) => return BundleSenderState::Idle,
        };

        if bundle.is_empty() {
            debug!("bundle is empty after proposal");
            return BundleSenderState::Idle;
        }

        for rejected in &bundle.rejected_ops {
            self.pool.remove_by_hash(&rejected.hash);
        }

        info!(
            ops = bundle.len(),
            gas_estimate = bundle.gas_estimate,
            "built bundle"
        );

        self.current_bundle = Some(bundle);
        BundleSenderState::Submitting
    }

    /// Submitting: sign and send the bundle transaction
    async fn handle_submitting(&mut self) -> BundleSenderState {
        let bundle = match &self.current_bundle {
            Some(bundle) => bundle,
            None => return BundleSenderState::Idle,
        };

        // Build the transaction
        let nonce = self.tracker.next_nonce();
        let tx = self.build_bundle_tx(bundle, nonce);

        let signed_tx = match self.signer.sign_transaction(tx, self.chain_spec.id).await {
            Ok(raw) => raw,
            Err(e) => {
                error!(error = %e, "failed to sign bundle transaction");
                return BundleSenderState::Abandoned;
            }
        };

        let max_fee = bundle.gas_fees.max_fee_per_gas;
        let max_priority_fee = bundle.gas_fees.max_priority_fee_per_gas;

        let ctx = SubmissionContext {
            expected_storage: None,
            chain_id: self.chain_spec.id,
        };

        match self.strategy.submit(signed_tx, &ctx).await {
            Ok(tx_hash) => {
                info!(
                    tx_hash = %tx_hash,
                    nonce,
                    ops = bundle.len(),
                    "bundle submitted"
                );
                self.tracker
                    .record_submission(tx_hash, nonce, max_fee, max_priority_fee);
                BundleSenderState::Pending
            }
            Err(e) => {
                error!(error = %e, "bundle submission failed");
                counter!("bunzo_bundles_failed_total").increment(1);
                BundleSenderState::Abandoned
            }
        }
    }

    /// Pending: poll the transaction tracker for mined/dropped status
    async fn handle_pending(&mut self) -> BundleSenderState {
        match self.tracker.check_pending().await {
            Ok(Some(TransactionStatus::Mined { block_number })) => {
                info!(block = block_number, "bundle transaction mined");
                self.mined_block = Some(block_number);
                // Remove bundled ops from the pool
                self.remove_bundled_ops();
                BundleSenderState::Confirming
            }
            Ok(Some(TransactionStatus::Dropped)) => {
                warn!("bundle transaction dropped from mempool");
                BundleSenderState::Abandoned
            }
            Ok(Some(TransactionStatus::Pending)) => {
                // Still pending. Check if we need to escalate
                if self.tracker.needs_escalation() {
                    BundleSenderState::Escalating
                } else {
                    BundleSenderState::Pending
                }
            }
            Ok(None) => {
                // No pending transaction shouldn't happen in this state
                warn!("no pending transaction in Pending state");
                BundleSenderState::Idle
            }
            Err(e) => {
                warn!(error = %e, "error checking pending transaction");
                BundleSenderState::Pending
            }
        }
    }

    /// Escalating: bump fees on the pending transaction
    async fn handle_escalating(&mut self) -> BundleSenderState {
        let (new_max_fee, new_priority_fee) = match self.tracker.escalated_fees().await {
            Some(fees) => fees,
            None => {
                warn!("max escalation attempts reached, abandoning");
                self.tracker.clear_pending();
                return BundleSenderState::Abandoned;
            }
        };

        let bundle = match &self.current_bundle {
            Some(b) => b,
            None => {
                error!("no bundle in escalating state");
                self.tracker.clear_pending();
                return BundleSenderState::Abandoned;
            }
        };

        // Build a replacement transaction with higher fees
        let nonce = self.tracker.next_nonce().saturating_sub(1);
        let mut tx = self.build_bundle_tx(bundle, nonce);
        tx = tx
            .max_fee_per_gas(new_max_fee)
            .max_priority_fee_per_gas(new_priority_fee);

        let signed_tx = match self.signer.sign_transaction(tx, self.chain_spec.id).await {
            Ok(raw) => raw,
            Err(e) => {
                error!(error = %e, "failed to sign escalated transaction");
                return BundleSenderState::Pending;
            }
        };

        let ctx = SubmissionContext {
            expected_storage: None,
            chain_id: self.chain_spec.id,
        };

        match self.strategy.submit(signed_tx, &ctx).await {
            Ok(new_tx_hash) => {
                info!(
                    tx_hash = %new_tx_hash,
                    max_fee = new_max_fee,
                    priority_fee = new_priority_fee,
                    "fee escalation submitted"
                );
                self.tracker
                    .record_escalation(new_tx_hash, new_max_fee, new_priority_fee);
                BundleSenderState::Pending
            }
            Err(e) => {
                warn!(error = %e, "fee escalation failed, will retry");
                BundleSenderState::Pending
            }
        }
    }

    /// Cancelling: send a cancellation transaction (zero-value self-transfer at same nonce)
    async fn handle_cancelling(&mut self) -> BundleSenderState {
        let pending = match self.tracker.pending() {
            Some(p) => p.clone(),
            None => return BundleSenderState::Idle,
        };

        let signer_addr = self.signer.address();
        let cancle_tx = alloy_rpc_types_eth::TransactionRequest::default()
            .to(signer_addr)
            .nonce(pending.nonce)
            .value(alloy_primitives::U256::ZERO)
            .gas_limit(21000)
            .max_fee_per_gas(pending.max_fee_per_gas * 2)
            .max_priority_fee_per_gas(pending.max_priority_fee_per_gas * 2);

        let signer = match self
            .signer
            .sign_transaction(cancle_tx, self.chain_spec.id)
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                error!(error = %e, "failed to sign cancellation");
                self.tracker.clear_pending();
                return BundleSenderState::Abandoned;
            }
        };

        match self.strategy.cancel(pending.tx_hash, Some(signer)).await {
            Ok(_) => {
                info!("cancellation submitted");
                self.tracker.clear_pending();
                BundleSenderState::Abandoned
            }
            Err(e) => {
                warn!(error = %e, "cancellation failed");
                self.tracker.clear_pending();
                BundleSenderState::Abandoned
            }
        }
    }

    /// Confirming: wait for sufficient confirmation depth
    async fn handle_confirming(&mut self) -> BundleSenderState {
        let mined_at = match self.mined_block {
            Some(b) => b,
            None => return BundleSenderState::Confirmed,
        };

        let current_block = match self.provider.get_block_number().await {
            Ok(n) => n,
            Err(e) => {
                warn!(error = %e, "failed to get block number for confirmation check");
                return BundleSenderState::Confirming;
            }
        };

        let confirmations = current_block.saturating_sub(mined_at);
        if confirmations >= self.confirmation_depth {
            info!(mined_at, current_block, confirmations, "bundle confirmed");
            BundleSenderState::Confirmed
        } else {
            debug!(
                mined_at,
                current_block,
                confirmations,
                target = self.confirmation_depth,
                "waiting for confirmations"
            );
            BundleSenderState::Confirming
        }
    }
    fn build_bundle_tx(
        &self,
        bundle: &Bundle,
        nonce: u64,
    ) -> alloy_rpc_types_eth::TransactionRequest {
        let calldata = {
            use alloy_sol_types::SolCall;
            let contract_ops = bundle
                .ops
                .iter()
                .map(|x| bunzo_provider::contracts::v0_7::PackedUserOperation {
                    sender: x.sender,
                    nonce: x.nonce,
                    initCode: x.initCode.clone(),
                    callData: x.callData.clone(),
                    accountGasLimits: x.accountGasLimits,
                    preVerificationGas: x.preVerificationGas,
                    gasFees: x.gasFees,
                    paymasterAndData: x.paymasterAndData.clone(),
                    signature: x.signature.clone(),
                })
                .collect();

            bunzo_provider::contracts::v0_7::IEntryPoint::handleOpsCall {
                ops: contract_ops,
                beneficiary: bundle.beneficiary,
            }
            .abi_encode()
        };

        alloy_rpc_types_eth::TransactionRequest::default()
            .to(self.chain_spec.entry_point_v0_7)
            .nonce(nonce)
            .gas_limit(bundle.gas_estimate)
            .max_fee_per_gas(bundle.gas_fees.max_fee_per_gas)
            .max_priority_fee_per_gas(bundle.gas_fees.max_priority_fee_per_gas)
            .input(Bytes::from(calldata).into())
    }

    fn remove_bundled_ops(&self) {
        let bundle = match &self.current_bundle {
            Some(bundle) => bundle,
            None => return,
        };

        for packed_op in &bundle.ops {
            let uo = UserOperation::unpack(
                packed_op,
                self.chain_spec.entry_point_v0_7,
                self.chain_spec.id,
            );
            let id = uo.id();
            self.pool.remove_by_id(&id);
        }
    }
}

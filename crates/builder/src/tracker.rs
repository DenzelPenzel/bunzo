use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_primitives::{Address, B256, U256};
use tracing::{debug, info, warn};

use bunzo_provider::traits::EvmProvider;

#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub tx_hash: B256,
    pub nonce: u64,
    pub escalation_count: u32,
    pub submitted_at: Instant,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
}

#[derive(Debug, Clone)]
pub enum TransactionStatus {
    Pending,
    Mined { block_number: u64 },
    Dropped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationStrategy {
    /// Fixed percentage bump per escalation round
    Linear,
    /// Query the network for current gas prices and set fees to market + premium
    /// Adapts in a single round during gas spikes instead of 5-10 rounds with fixed bumps
    NetworkTracking,
}

#[derive(Debug, Clone)]
pub struct EscalationConfig {
    /// How long to wait before the first fee escalation
    pub initial_wait: Duration,
    /// Minimum fee bump in basis points per escalation (10000 = 100%)
    /// Also used as the minimum required bump for network-tracking mode
    pub fee_bump_bps: u32,
    /// Maximum number of escalation attempts before abandoning
    pub max_attempts: u32,
    /// Which escalation strategy to use
    pub strategy: EscalationStrategy,
    /// Premium in basis points to add on top of market fees in network-tracking mode
    /// For example, 500 = 5% above current market price
    pub network_premium_bps: u32,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            initial_wait: Duration::from_secs(30),
            fee_bump_bps: 1250, // 12.5%
            max_attempts: 5,
            strategy: EscalationStrategy::NetworkTracking,
            network_premium_bps: 1500, // 15% above market
        }
    }
}

pub struct TransactionTracker<P> {
    provider: Arc<P>,
    signer_address: Address,
    pending: Option<PendingTransaction>,
    current_nonce: u64,
    escalation_config: EscalationConfig,
}

impl<P: EvmProvider> TransactionTracker<P> {
    pub fn new(
        provider: Arc<P>,
        signer_address: Address,
        escalation_config: EscalationConfig,
    ) -> Self {
        Self {
            provider,
            signer_address,
            pending: None,
            current_nonce: 0,
            escalation_config,
        }
    }

    pub async fn initialize(&mut self) -> Result<(), bunzo_provider::ProviderError> {
        self.current_nonce = self.provider.get_nonce(self.signer_address).await?;
        info!(
            address = %self.signer_address,
            nonce = self.current_nonce,
            "transaction tracker initialized"
        );
        Ok(())
    }

    pub fn next_nonce(&self) -> u64 {
        self.current_nonce
    }

    pub async fn balance(&self) -> Result<U256, bunzo_provider::ProviderError> {
        self.provider.get_balance(self.signer_address).await
    }

    pub fn record_submission(
        &mut self,
        tx_hash: B256,
        nonce: u64,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
    ) {
        info!(
            tx_hash = %tx_hash,
            nonce,
            "recorded pending transaction"
        );
        self.pending = Some(PendingTransaction {
            tx_hash,
            nonce,
            escalation_count: 0,
            submitted_at: Instant::now(),
            max_fee_per_gas,
            max_priority_fee_per_gas,
        });
    }

    pub async fn check_pending(
        &mut self,
    ) -> Result<Option<TransactionStatus>, bunzo_provider::ProviderError> {
        let pending = match &self.pending {
            Some(p) => p.clone(),
            None => return Ok(None),
        };

        let receipt = self
            .provider
            .get_transaction_receipt(pending.tx_hash)
            .await?;

        if let Some(receipt) = receipt {
            let block_number = receipt.block_number.unwrap_or(0);
            info!(
                tx_hash = %pending.tx_hash,
                block = block_number,
                "transaction mined"
            );
            self.current_nonce = pending.nonce + 1;
            self.pending = None;
            return Ok(Some(TransactionStatus::Mined { block_number }));
        }

        let chain_nonce = self.provider.get_nonce(self.signer_address).await?;

        if chain_nonce > pending.nonce {
            warn!(
                tx_hash = %pending.tx_hash,
                expected_nonce = pending.nonce,
                chain_nonce,
                "transaction nonce consumed (replaced or mined by other)"
            );
            self.current_nonce = chain_nonce;
            self.pending = None;
            return Ok(Some(TransactionStatus::Dropped));
        }

        Ok(Some(TransactionStatus::Pending))
    }

    /// Check if the pending transaction needs fee escalation
    pub fn needs_escalation(&self) -> bool {
        match &self.pending {
            Some(p) => {
                p.submitted_at.elapsed() >= self.escalation_config.initial_wait
                    && p.escalation_count < self.escalation_config.max_attempts
            }
            None => false,
        }
    }

    /// Get the escalated fee parameters using the configured strategy
    ///
    /// For `Linear`: applies a fixed percentage bump
    /// For `NetworkTracking`: queries the network and sets fees to market + premium,
    /// but ensures at least the minimum required bump for nonce replacement
    ///
    /// Returns `(new_max_fee, new_max_priority_fee)` or `None` if escalation
    /// is not possible
    pub async fn escalated_fees(&mut self) -> Option<(u128, u128)> {
        let pending = self.pending.as_ref()?;

        if pending.escalation_count >= self.escalation_config.max_attempts {
            return None;
        }

        match self.escalation_config.strategy {
            EscalationStrategy::Linear => {
                let bump_bps = self.escalation_config.fee_bump_bps;
                let multiplier = 10000u128 + bump_bps as u128;
                let new_max_fee = pending.max_fee_per_gas * multiplier / 10000;
                let new_priority_fee = pending.max_priority_fee_per_gas * multiplier / 10000;
                Some((new_max_fee, new_priority_fee))
            }
            EscalationStrategy::NetworkTracking => self.escalated_fees_network().await,
        }
    }

    /// Network-tracking escalation: query current market fees and apply a premium
    async fn escalated_fees_network(&self) -> Option<(u128, u128)> {
        let pending = self.pending.as_ref()?;

        // Fetch current network fees
        let base_fee = self.provider.get_base_fee().await.ok()?;
        let priority_fee = self.provider.get_max_priority_fee().await.ok()?;

        let premium_multiplier = 10000u128 + self.escalation_config.network_premium_bps as u128;
        let market_max_fee = (base_fee + priority_fee) * premium_multiplier / 10000;
        let market_priority = priority_fee * premium_multiplier / 10000;

        // Ensure at least the minimum required bump for nonce replacement
        // (Ethereum requires ~10% bump for replacement transactions)
        let min_bump_multiplier = 10000u128 + self.escalation_config.fee_bump_bps as u128;
        let min_max_fee = pending.max_fee_per_gas * min_bump_multiplier / 10000;
        let min_priority = pending.max_priority_fee_per_gas * min_bump_multiplier / 10000;

        let new_max_fee = market_max_fee.max(min_max_fee);
        let new_priority_fee = market_priority.max(min_priority);
        Some((new_max_fee, new_priority_fee))
    }

    pub fn record_escalation(
        &mut self,
        new_tx_hash: B256,
        new_max_fee: u128,
        new_priority_fee: u128,
    ) {
        if let Some(pending) = &mut self.pending {
            debug!(
                old_hash = %pending.tx_hash,
                new_hash = %new_tx_hash,
                attempt = pending.escalation_count + 1,
                "fee escalation recorded"
            );
            pending.tx_hash = new_tx_hash;
            pending.max_fee_per_gas = new_max_fee;
            pending.max_priority_fee_per_gas = new_priority_fee;
            pending.escalation_count += 1;
        }
    }

    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn pending(&self) -> Option<&PendingTransaction> {
        self.pending.as_ref()
    }
}

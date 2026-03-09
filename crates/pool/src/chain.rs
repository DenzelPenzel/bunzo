use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alloy_primitives::B256;
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, warn};

use bunzo_provider::traits::EvmProvider;
use bunzo_types::chain::ChainSpec;
use bunzo_types::event::{ChainEvent, EventBus};
use bunzo_types::user_operation::UserOperation as UserOperationTrait;

use crate::pool::OperationPool;
use crate::reputation::ReputationManager;

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct BlockEntry {
    number: u64,
    hash: B256,
    base_fee: u128,
}

/// Watches the chain for new blocks and mined user op
pub struct ChainWatcher<P> {
    provider: Arc<P>,
    pool: Arc<OperationPool>,
    reputation: Arc<ReputationManager>,
    chain_spec: ChainSpec,
    event_bus: Arc<EventBus<ChainEvent>>,
    base_fee: Arc<AtomicU64>,
    block_history: VecDeque<BlockEntry>,
    last_block: u64,
    poll_interval: Duration,
}

impl<P: EvmProvider> ChainWatcher<P> {
    pub fn new(
        provider: Arc<P>,
        pool: Arc<OperationPool>,
        reputation: Arc<ReputationManager>,
        chain_spec: ChainSpec,
        event_bus: Arc<EventBus<ChainEvent>>,
        base_fee: Arc<AtomicU64>,
    ) -> Self {
        let history_size = chain_spec.chain_history_size as usize;

        Self {
            provider,
            pool,
            reputation,
            chain_spec,
            event_bus,
            base_fee,
            block_history: VecDeque::with_capacity(history_size),
            last_block: 0,
            // TODO: make it configurable
            poll_interval: Duration::from_secs(60),
        }
    }

    pub async fn run(mut self, mut shutdown: tokio::sync::watch::Receiver<()>) {
        match self.provider.get_block_number().await {
            Ok(num) => {
                self.last_block = num;
                info!(block = num, "chain watcher initialized");
            }
            Err(e) => {
                error!(error = %e, "failed to get initial block number");
            }
        }

        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.pool().await {
                        warn!(error = %e, "chain watcher poll error");
                    }
                }
                _ = shutdown.changed() => {
                    info!("chain watcher received shutdown signal");
                    break;
                }
            }
        }

        info!("chain watcher stopped");
    }

    async fn pool(&mut self) -> Result<(), bunzo_provider::ProviderError> {
        let current_block = self.provider.get_block_number().await?;

        if current_block <= self.last_block {
            return Ok(());
        }

        for block_num in (self.last_block + 1)..=current_block {
            self.process_block(block_num).await?;
        }

        self.last_block = current_block;

        Ok(())
    }

    async fn process_block(
        &mut self,
        block_number: u64,
    ) -> Result<(), bunzo_provider::ProviderError> {
        let block = self
            .provider
            .get_block(block_number)
            .await?
            .ok_or_else(|| {
                bunzo_provider::ProviderError::InvalidResponse(format!(
                    "block {block_number} not found"
                ))
            })?;

        let block_hash = block.header.hash;
        let base_fee = block.header.base_fee_per_gas.unwrap_or(0) as u128;
        let ts = block.header.timestamp;

        self.base_fee.store(base_fee as u64, Ordering::Relaxed);

        if let Some(last_entry) = self.block_history.back() {
            if block.header.parent_hash != last_entry.hash && block_number == last_entry.number + 1
            {
                let reorg_depth = self.detect_reorg_depth(&block.header.parent_hash);
                if reorg_depth > 0 {
                    warn!(
                        depth = reorg_depth,
                        block = block_number,
                        "chain reorg detected"
                    );

                    self.handle_reorg(reorg_depth);
                    self.event_bus.publish(ChainEvent::Reorg {
                        depth: reorg_depth,
                        new_head: block_number,
                    });
                }
            }
        }

        self.scan_user_op_events(block_number).await?;

        let entry = BlockEntry {
            number: block_number,
            hash: block_hash,
            base_fee,
        };
        self.block_history.push_back(entry);

        let max_history = self.chain_spec.chain_history_size as usize;
        while self.block_history.len() > max_history {
            self.block_history.pop_front();
        }

        self.event_bus.publish(ChainEvent::NewBlock {
            number: block_number,
            hash: block_hash,
            base_fee,
            timestamp: ts,
        });

        debug!(
            block = block_number,
            base_fee,
            pool_size = self.pool.size(),
            "processed block"
        );

        Ok(())
    }

    async fn scan_user_op_events(
        &self,
        block_number: u64,
    ) -> Result<(), bunzo_provider::ProviderError> {
        let user_op_event_topic = alloy_primitives::keccak256(
            "UserOperationEvent(bytes32,address,address,uint256,bool,uint256,uint256)",
        );

        let filter = alloy_rpc_types_eth::Filter::new()
            .address(self.chain_spec.entry_point_v0_7)
            .event_signature(user_op_event_topic)
            .from_block(block_number)
            .to_block(block_number);

        let logs = self.provider.get_logs(&filter).await?;

        for log in &logs {
            if log.topics().len() < 2 {
                continue;
            }

            let user_op_hash = log.topics()[1];

            if let Some(entry) = self.pool.remove_by_hash(&user_op_hash) {
                info!(
                    hash = %user_op_hash,
                    sender = %entry.uo.sender,
                    block = block_number,
                    "mined user operation removed from pool"
                );

                let entities = entry.uo.entities();
                for entity in &entities {
                    self.reputation.record_included(&entity.address);
                }
            } else {
                debug!(
                    hash = %user_op_hash,
                    block = block_number,
                    "mined op not found in pool (already removed or unknown)"
                );
            }
        }

        if !logs.is_empty() {
            debug!(
                block = block_number,
                events = logs.len(),
                "processed UserOperationEvent logs"
            );
        }

        Ok(())
    }

    /// Determine how deep a reorg goes by walking back through block history
    fn detect_reorg_depth(&self, new_parent_hash: &B256) -> u64 {
        for (i, entry) in self.block_history.iter().rev().enumerate() {
            if entry.hash == *new_parent_hash {
                return i as u64;
            }
        }
        // reorg is deeper than our history window
        self.block_history.len() as u64
    }

    /// Handle a chain reorganization by removing blocks from history
    /// TODO: re-add the user operations from the reorged blocks back into the pool
    fn handle_reorg(&mut self, depth: u64) {
        for _ in 0..depth {
            self.block_history.pop_back();
        }
        if let Some(last) = self.block_history.back() {
            self.last_block = last.number;
        }
    }
}

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;

use bunzo_types::gas::GasFees;

use crate::error::ProviderResult;
use crate::traits::EvmProvider;

#[async_trait]
pub trait GasOracle: Send + Sync + 'static {
    async fn current_fees(&self) -> ProviderResult<GasFees>;
    async fn base_fee(&self) -> ProviderResult<u128>;
}

#[derive(Debug, Clone)]
struct BlockFees {
    block_number: u64,
    base_fee: u128,
    priority_fee: u128,
}

/// Gas oracle with fee history tracking and per-block caching
pub struct ProviderGasOracle<P> {
    provider: P,
    last_base_fee: AtomicU64,
    last_block: AtomicU64,
    fee_history: Mutex<VecDeque<BlockFees>>,
    history_size: usize,
}

impl<P: EvmProvider> ProviderGasOracle<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            last_base_fee: AtomicU64::new(0),
            last_block: AtomicU64::new(0),
            fee_history: Mutex::new(VecDeque::with_capacity(5)),
            history_size: 5,
        }
    }

    /// Refresh the fee cache if a new block has been produced
    async fn refresh_if_needed(&self) -> ProviderResult<()> {
        let current_block = self.provider.get_block_number().await?;
        let cached_block = self.last_block.load(Ordering::Relaxed);

        if current_block <= cached_block {
            return Ok(());
        }

        let base_fee = self.provider.get_base_fee().await?;
        let priority_fee = self.provider.get_max_priority_fee().await?;

        self.last_base_fee.store(base_fee as u64, Ordering::Relaxed);
        self.last_block.store(current_block, Ordering::Relaxed);

        let mut history = self.fee_history.lock().unwrap();
        history.push_back(BlockFees {
            block_number: current_block,
            base_fee,
            priority_fee,
        });

        while history.len() > self.history_size {
            history.pop_front();
        }

        Ok(())
    }

    fn median_priority_fee(&self) -> Option<u128> {
        let history = self.fee_history.lock().unwrap();

        if history.is_empty() {
            return None;
        }

        let mut fees: Vec<u128> = history.iter().map(|x| x.priority_fee).collect();
        fees.sort_unstable();
        let mid = fees.len() >> 1;
        Some(fees[mid])
    }
}

#[async_trait]
impl<P: EvmProvider> GasOracle for ProviderGasOracle<P> {
    async fn current_fees(&self) -> ProviderResult<GasFees> {
        self.refresh_if_needed().await?;

        let base_fee = self.last_base_fee.load(Ordering::Relaxed) as u128;

        let priority_fee = self.median_priority_fee().unwrap_or_else(|| base_fee / 10);

        Ok(GasFees::new(base_fee + priority_fee, priority_fee))
    }

    async fn base_fee(&self) -> ProviderResult<u128> {
        self.refresh_if_needed().await?;

        let cached = self.last_base_fee.load(Ordering::Relaxed);
        if cached > 0 {
            return Ok(cached as u128);
        }
        self.provider.get_base_fee().await
    }
}

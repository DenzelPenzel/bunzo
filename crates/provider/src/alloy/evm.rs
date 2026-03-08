use alloy_eips::BlockNumberOrTag;
use alloy_primitives::{Address, B256, Bytes, U256};
use alloy_provider::Provider;
use alloy_rpc_types_eth::{Block, Filter, Log, TransactionReceipt, TransactionRequest};
use async_trait::async_trait;

use crate::error::{ProviderError, ProviderResult};
use crate::traits::EvmProvider;

pub struct AlloyEvmProvider<P> {
    inner: P,
}

impl<P> AlloyEvmProvider<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<P> EvmProvider for AlloyEvmProvider<P>
where
    P: Provider + Send + Sync + 'static,
{
    async fn call(&self, tx: &TransactionRequest, block: Option<u64>) -> ProviderResult<Bytes> {
        let block_id = block
            .map(BlockNumberOrTag::Number)
            .unwrap_or(BlockNumberOrTag::Latest);
        self.inner
            .call(tx.clone())
            .block(block_id.into())
            .await
            .map(|r| Bytes::from(r.to_vec()))
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_logs(&self, filter: &Filter) -> ProviderResult<Vec<Log>> {
        self.inner
            .get_logs(filter)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_block(&self, block_number: u64) -> ProviderResult<Option<Block>> {
        self.inner
            .get_block_by_number(BlockNumberOrTag::Number(block_number))
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_block_number(&self) -> ProviderResult<u64> {
        self.inner
            .get_block_number()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_nonce(&self, address: Address) -> ProviderResult<u64> {
        self.inner
            .get_transaction_count(address)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_balance(&self, address: Address) -> ProviderResult<U256> {
        self.inner
            .get_balance(address)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn send_raw_transaction(&self, raw_tx: Bytes) -> ProviderResult<B256> {
        let pending = self
            .inner
            .send_raw_transaction(&raw_tx)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        Ok(*pending.tx_hash())
    }

    async fn get_transaction_receipt(
        &self,
        tx_hash: B256,
    ) -> ProviderResult<Option<TransactionReceipt>> {
        self.inner
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_chain_id(&self) -> ProviderResult<u64> {
        self.inner
            .get_chain_id()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }

    async fn get_base_fee(&self) -> ProviderResult<u128> {
        let block = self
            .inner
            .get_block_by_number(BlockNumberOrTag::Latest)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?
            .ok_or_else(|| ProviderError::InvalidResponse("no latest block".into()))?;
        block
            .header
            .base_fee_per_gas
            .map(|f| f as u128)
            .ok_or_else(|| ProviderError::InvalidResponse("block missing base_fee_per_gas".into()))
    }

    async fn get_max_priority_fee(&self) -> ProviderResult<u128> {
        self.inner
            .get_max_priority_fee_per_gas()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))
    }
}

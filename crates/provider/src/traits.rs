use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_rpc_types_eth::{Block, Filter, Log, TransactionReceipt, TransactionRequest};
use async_trait::async_trait;

use bunzo_types::gas::GasFees;
use bunzo_types::user_operation::v0_7::PackedUserOperation;
use bunzo_types::validation::{ValidationOutput, ValidationRevert};

use crate::error::ProviderResult;

#[async_trait]
pub trait EvmProvider: Send + Sync + 'static {
    /// Execute a call without creating a transaction
    async fn call(&self, tx: &TransactionRequest, block: Option<u64>) -> ProviderResult<Bytes>;

    /// Get logs matching a filter
    async fn get_logs(&self, filter: &Filter) -> ProviderResult<Vec<Log>>;

    /// Get a block by number
    async fn get_block(&self, block_number: u64) -> ProviderResult<Option<Block>>;

    /// Get the latest block number
    async fn get_block_number(&self) -> ProviderResult<u64>;

    /// Get the nonce
    async fn get_nonce(&self, address: Address) -> ProviderResult<u64>;

    /// Get the balance of an address
    async fn get_balance(&self, address: Address) -> ProviderResult<U256>;

    /// Send a raw signed transaction
    async fn send_raw_transaction(&self, raw_tx: Bytes) -> ProviderResult<B256>;

    /// Get a transaction receipt by hash
    async fn get_transaction_receipt(
        &self,
        tx_hash: B256,
    ) -> ProviderResult<Option<TransactionReceipt>>;

    async fn get_chain_id(&self) -> ProviderResult<u64>;

    /// Get the current base fee per gas
    async fn get_base_fee(&self) -> ProviderResult<u128>;

    /// Get the suggested max priority fee per gas
    async fn get_max_priority_fee(&self) -> ProviderResult<u128>;
}

#[async_trait]
pub trait EntryPointProvider: Send + Sync + 'static {
    /// Simulate validation of a user operation
    async fn simulate_validation(
        &self,
        user_op: PackedUserOperation,
    ) -> ProviderResult<Result<ValidationOutput, ValidationRevert>>;

    /// Simulate execution of a user operation via `simulateHandleOp`
    async fn simulate_handle_op(
        &self,
        user_op: PackedUserOperation,
        target: Address,
        target_call_data: Bytes,
    ) -> ProviderResult<Result<ExecutionResult, ValidationRevert>>;

    /// Encode `handleOps` calldata for a batch of operations
    fn encode_handle_ops(&self, ops: Vec<PackedUserOperation>, beneficiary: Address) -> Bytes;

    /// Get the user operation hash from the entry point
    async fn get_user_op_hash(&self, user_op: PackedUserOperation) -> ProviderResult<B256>;

    /// Get the deposit balance for an address
    async fn get_balance_of(&self, address: Address) -> ProviderResult<U256>;
}

/// Result of simulating a handleOp call
#[derive(Debug, Clone, Default)]
pub struct ExecutionResult {
    pub pre_op_gas: u128,
    pub paid: U256,
    pub target_success: bool,
    pub target_result: Bytes,
}

/// Result of calling handleOps on the entry point
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleOpsOut {
    Success,
    FailedOp(usize, String),
    SignatureValidationFailed(Address),
    Revert(Bytes),
}

#[async_trait]
impl EvmProvider for () {
    async fn call(&self, _tx: &TransactionRequest, _block: Option<u64>) -> ProviderResult<Bytes> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_logs(&self, _filter: &Filter) -> ProviderResult<Vec<Log>> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_block(&self, _block_number: u64) -> ProviderResult<Option<Block>> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_block_number(&self) -> ProviderResult<u64> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_nonce(&self, _address: Address) -> ProviderResult<u64> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_balance(&self, _address: Address) -> ProviderResult<U256> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn send_raw_transaction(&self, _raw_tx: Bytes) -> ProviderResult<B256> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_transaction_receipt(
        &self,
        _tx_hash: B256,
    ) -> ProviderResult<Option<TransactionReceipt>> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_chain_id(&self) -> ProviderResult<u64> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_base_fee(&self) -> ProviderResult<u128> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
    async fn get_max_priority_fee(&self) -> ProviderResult<u128> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no provider configured"
        )))
    }
}

#[async_trait]
impl EntryPointProvider for () {
    async fn simulate_validation(
        &self,
        _user_op: PackedUserOperation,
    ) -> ProviderResult<Result<ValidationOutput, ValidationRevert>> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no entry point provider configured"
        )))
    }
    async fn simulate_handle_op(
        &self,
        _user_op: PackedUserOperation,
        _target: Address,
        _target_call_data: Bytes,
    ) -> ProviderResult<Result<ExecutionResult, ValidationRevert>> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no entry point provider configured"
        )))
    }
    fn encode_handle_ops(&self, _ops: Vec<PackedUserOperation>, _beneficiary: Address) -> Bytes {
        Bytes::new()
    }
    async fn get_user_op_hash(&self, _user_op: PackedUserOperation) -> ProviderResult<B256> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no entry point provider configured"
        )))
    }
    async fn get_balance_of(&self, _address: Address) -> ProviderResult<U256> {
        Err(crate::ProviderError::Other(anyhow::anyhow!(
            "no entry point provider configured"
        )))
    }
}

#[async_trait]
pub trait BundleHandler: Send + Sync + 'static {
    async fn call_handle_ops(
        &self,
        ops: Vec<PackedUserOperation>,
        beneficiary: Address,
        gas_limit: u64,
        gas_fees: GasFees,
    ) -> ProviderResult<HandleOpsOut>;

    fn build_handle_ops_tx(
        &self,
        ops: Vec<PackedUserOperation>,
        beneficiary: Address,
        gas_limit: u64,
        gas_fees: GasFees,
    ) -> TransactionRequest;
}

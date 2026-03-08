use std::sync::Arc;

use alloy_primitives::{Address, B256, U256};
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use bunzo_pool::{OperationPool, Validator};
use bunzo_provider::traits::{EntryPointProvider, EvmProvider};
use bunzo_types::chain::ChainSpec;
use bunzo_types::gas::GasEstimate;
use bunzo_types::user_operation::UserOperation as UserOperationTrait;
use bunzo_types::user_operation::v0_7::{UserOperation, UserOperationOptionalGas};

use crate::error::RpcError;

#[rpc(server, namespace = "eth")]
pub trait EthApi {
    #[method(name = "sendUserOperation")]
    async fn send_user_operation(
        &self,
        user_op: UserOperation,
        entry_point: Address,
    ) -> RpcResult<B256>;

    #[method(name = "estimateUserOperationGas")]
    async fn estimate_user_operation_gas(
        &self,
        user_op: UserOperationOptionalGas,
        entry_point: Address,
    ) -> RpcResult<GasEstimate>;

    #[method(name = "getUserOperationByHash")]
    async fn get_user_operation_by_hash(
        &self,
        hash: B256,
    ) -> RpcResult<Option<UserOperationWithEntryPoint>>;

    #[method(name = "getUserOperationReceipt")]
    async fn get_user_operation_receipt(
        &self,
        hash: B256,
    ) -> RpcResult<Option<UserOperationReceipt>>;

    #[method(name = "chainId")]
    async fn chain_id(&self) -> RpcResult<String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOperationWithEntryPoint {
    pub user_operation: UserOperation,
    pub entry_point: Address,
    pub block_number: Option<u64>,
    pub block_hash: Option<B256>,
    pub transaction_hash: Option<B256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOperationReceipt {
    pub user_op_hash: B256,
    pub sender: Address,
    pub nonce: U256,
    pub paymaster: Option<Address>,
    pub actual_gas_cost: U256,
    pub actual_gas_used: U256,
    pub success: bool,
    pub reason: String,
    pub receipt: serde_json::Value,
}

pub struct EthApiImpl<P = (), E = ()> {
    chain_spec: ChainSpec,
    pool: Arc<OperationPool>,
    validator: Arc<Validator>,
    base_fee: std::sync::atomic::AtomicU64,
    provider: Option<Arc<P>>,
    entry_point_provider: Option<Arc<E>>,
}

impl EthApiImpl<(), ()> {
    pub fn new(chain_spec: ChainSpec, pool: Arc<OperationPool>, validator: Arc<Validator>) -> Self {
        Self {
            chain_spec,
            pool,
            validator,
            base_fee: std::sync::atomic::AtomicU64::new(0),
            provider: None,
            entry_point_provider: None,
        }
    }
}

impl<P, E> EthApiImpl<P, E> {
    pub fn with_providers(
        chain_spec: ChainSpec,
        pool: Arc<OperationPool>,
        validator: Arc<Validator>,
        provider: Arc<P>,
        entry_point_provider: Arc<E>,
    ) -> Self {
        Self {
            chain_spec,
            pool,
            validator,
            base_fee: std::sync::atomic::AtomicU64::new(0),
            provider: Some(provider),
            entry_point_provider: Some(entry_point_provider),
        }
    }

    pub fn set_base_fee(&self, base_fee: u128) {
        self.base_fee
            .store(base_fee as u64, std::sync::atomic::Ordering::Relaxed);
    }

    fn current_base_fee(&self) -> u128 {
        self.base_fee.load(std::sync::atomic::Ordering::Relaxed) as u128
    }
}

#[async_trait::async_trait]
impl<P, E> EthApiServer for EthApiImpl<P, E>
where
    P: EvmProvider,
    E: EntryPointProvider,
{
    async fn send_user_operation(
        &self,
        mut user_op: UserOperation,
        entry_point: Address,
    ) -> RpcResult<B256> {
        if !self.chain_spec.is_entry_point(&entry_point) {
            return Err(
                RpcError::InvalidParams(format!("unsupported entry point: {entry_point}")).into(),
            );
        }

        user_op.set_context(entry_point, self.chain_spec.id);

        let base_fee = self.current_base_fee();

        self.validator
            .validate_sync(&user_op, base_fee)
            .map_err(|e| {
                warn!(error = %e, sender = %user_op.sender(), "user operation validation failed");
                RpcError::InvalidParams(e.to_string())
            })?;

        let hash = self.pool.add(user_op, base_fee).map_err(|e| {
            warn!(error = %e, "failed to add user operation to pool");
            RpcError::Pool(e)
        })?;

        info!(hash = %hash, "accepted user operation");

        Ok(hash)
    }

    async fn estimate_user_operation_gas(
        &self,
        user_op: UserOperationOptionalGas,
        entry_point: Address,
    ) -> RpcResult<GasEstimate> {
        if !self.chain_spec.is_entry_point(&entry_point) {
            return Err(
                RpcError::InvalidParams(format!("unsupported entry point: {entry_point}")).into(),
            );
        }

        let ep = self.entry_point_provider.as_ref().ok_or_else(|| {
            RpcError::Internal("provider not configured for gas estimation".into())
        })?;

        let max_verification_gas = 1_000_000u128;
        let max_call_gas = 10_000_000u128;
        let full_op = user_op.into_user_operation(
            entry_point,
            self.chain_spec.id,
            max_verification_gas,
            max_call_gas,
        );

        let packed = full_op.pack();
        let validation_res = ep
            .simulate_validation(packed.clone())
            .await
            .map_err(|e| RpcError::Internal(format!("simulation failed: {e}")))?
            .map_err(|e| RpcError::InvalidParams(format!("validation reverted: {e}")))?;

        let pre_op_gas = validation_res.pre_op_gas;
        let verification_gas_limit = pre_op_gas;

        let res = ep
            .simulate_handle_op(packed, Address::ZERO, alloy_primitives::Bytes::new())
            .await
            .map_err(|e| RpcError::Internal(format!("simulation failed: {e}")))?
            .map_err(|e| RpcError::InvalidParams(format!("execution reverted: {e}")))?;

        // Call gas = total gas used - pre-op gas
        let call_gas_limit = if res.pre_op_gas > pre_op_gas {
            res.pre_op_gas - pre_op_gas
        } else {
            res.paid
                .to::<u128>()
                .saturating_div(self.current_base_fee().max(1))
        };

        // calldata cost + intrinsic overhead
        let pre_verification_gas = {
            let calldata_cost = full_op.calldata_gas_cost(
                self.chain_spec.calldata_zero_byte_gas,
                self.chain_spec.calldata_non_zero_byte_gas,
            );
            calldata_cost + self.chain_spec.per_user_op_v0_7_gas as u128
        };

        let paymaster_verification_gas_limit = if full_op.paymaster.is_some() {
            Some(verification_gas_limit)
        } else {
            None
        };

        Ok(GasEstimate {
            pre_verification_gas,
            call_gas_limit,
            verification_gas_limit,
            paymaster_verification_gas_limit,
        })
    }

    async fn get_user_operation_by_hash(
        &self,
        hash: B256,
    ) -> RpcResult<Option<UserOperationWithEntryPoint>> {
        if let Some(entry) = self.pool.get_by_hash(&hash) {
            return Ok(Some(UserOperationWithEntryPoint {
                user_operation: entry.uo.clone(),
                entry_point: self.chain_spec.entry_point_v0_7,
                block_number: None,
                block_hash: None,
                transaction_hash: None,
            }));
        }

        Ok(None)
    }

    async fn get_user_operation_receipt(
        &self,
        hash: B256,
    ) -> RpcResult<Option<UserOperationReceipt>> {
        let provider = match self.provider.as_ref() {
            Some(p) => p,
            None => return Ok(None),
        };

        let user_op_event_topic = alloy_primitives::keccak256(
            "UserOperationEvent(bytes32,address,address,uint256,bool,uint256,uint256)",
        );

        let current_block = provider
            .get_block_number()
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        let from_block = current_block.saturating_sub(1000);

        let filter = alloy_rpc_types_eth::Filter::new()
            .address(self.chain_spec.entry_point_v0_7)
            .event_signature(user_op_event_topic)
            .topic1(hash)
            .from_block(from_block)
            .to_block(current_block);

        let logs = provider
            .get_logs(&filter)
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        let log = match logs.first() {
            Some(log) => log,
            None => return Ok(None),
        };

        // Decode the event data
        // Non-indexed data: nonce (u256), success (bool), actualGasCost (u256), actualGasUsed (u256)
        let topics = log.topics();
        let sender = if topics.len() > 2 {
            Address::from_slice(&topics[2].as_slice()[12..])
        } else {
            Address::ZERO
        };

        let paymaster = if topics.len() > 3 {
            let pm = Address::from_slice(&topics[3].as_slice()[12..]);
            if pm == Address::ZERO { None } else { Some(pm) }
        } else {
            None
        };

        let data = &log.data().data;
        let (nonce, success, actual_gas_cost, actual_gas_used) = if data.len() >= 128 {
            let nonce = U256::from_be_slice(&data[0..32]);
            let success = data[63] != 0;
            let actual_gas_cost = U256::from_be_slice(&data[64..96]);
            let actual_gas_used = U256::from_be_slice(&data[96..128]);
            (nonce, success, actual_gas_cost, actual_gas_used)
        } else {
            (U256::ZERO, false, U256::ZERO, U256::ZERO)
        };

        let tx_hash = log.transaction_hash.unwrap_or(B256::ZERO);
        let tx_receipt = if tx_hash != B256::ZERO {
            provider
                .get_transaction_receipt(tx_hash)
                .await
                .map_err(|e: bunzo_provider::ProviderError| RpcError::Internal(e.to_string()))?
        } else {
            None
        };

        let receipt_json = tx_receipt
            .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
            .unwrap_or(serde_json::Value::Null);

        Ok(Some(UserOperationReceipt {
            user_op_hash: hash,
            sender,
            nonce,
            paymaster,
            actual_gas_cost,
            actual_gas_used,
            success,
            reason: String::new(),
            receipt: receipt_json,
        }))
    }

    async fn chain_id(&self) -> RpcResult<String> {
        Ok(format!("{:#x}", self.chain_spec.id))
    }
}

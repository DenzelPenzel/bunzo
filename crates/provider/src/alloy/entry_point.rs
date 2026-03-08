use alloy_primitives::{Address, B256, Bytes, U256};
use alloy_provider::Provider;
use alloy_sol_types::{SolCall, SolInterface};
use async_trait::async_trait;

use bunzo_types::user_operation::v0_7::PackedUserOperation;
use bunzo_types::validation::{StakeInfo, ValidationOutput, ValidationRevert};

use crate::contracts::v0_7::{IEntryPoint, IEntryPointSimulations};
use crate::error::{ProviderError, ProviderResult};
use crate::traits::{EntryPointProvider, ExecutionResult};

/// Parse the packed validation data from the EntryPoint
/// Layout in uint256: `aggregator/sigFailed (160 bits) | validUntil (48 bits) | validAfter (48 bits)`
fn parse_validation_data(data: U256) -> (bool, u64, u64) {
    let mask_48: U256 = U256::from(0xFFFFFFFFFFFFu64);
    let valid_after: u64 = (data & mask_48).to::<u64>();
    let shifted: U256 = data >> 48;
    let valid_until = (shifted & mask_48).to::<u64>();
    let sig: U256 = data >> 160;
    let sig_failed = sig == U256::from(1);
    (sig_failed, valid_after, valid_until)
}

pub struct AlloyEntryPointProvider<P> {
    provider: P,
    entry_point: Address,
}

impl<P> AlloyEntryPointProvider<P> {
    pub fn new(provider: P, entry_point: Address) -> Self {
        Self {
            provider,
            entry_point,
        }
    }
}

#[async_trait]
impl<P> EntryPointProvider for AlloyEntryPointProvider<P>
where
    P: Provider + Send + Sync + 'static,
{
    async fn simulate_validation(
        &self,
        user_op: PackedUserOperation,
    ) -> ProviderResult<Result<ValidationOutput, ValidationRevert>> {
        let packed = convert_packed_uo(&user_op);
        let calldata =
            IEntryPointSimulations::simulateValidationCall { userOp: packed }.abi_encode();

        let tx = alloy_rpc_types_eth::TransactionRequest::default()
            .to(self.entry_point)
            .input(Bytes::from(calldata).into());

        let res = self.provider.call(tx).await;

        match res {
            Ok(output) => {
                match IEntryPointSimulations::simulateValidationCall::abi_decode_returns(&output) {
                    Ok(vr) => {
                        let (account_sig_failed, valid_after, valid_until) =
                            parse_validation_data(vr.returnInfo.accountValidationData);
                        let (paymaster_sig_failed, _, _) =
                            parse_validation_data(vr.returnInfo.paymasterValidationData);

                        Ok(Ok(ValidationOutput {
                            pre_op_gas: vr.returnInfo.preOpGas.to::<u128>(),
                            prefund: vr.returnInfo.prefund,
                            valid_after,
                            valid_until,
                            account_sig_failed,
                            paymaster_sig_failed,
                            sender_info: StakeInfo {
                                stake: vr.senderInfo.stake,
                                unstake_delay_sec: vr.senderInfo.unstakeDelaySec.to::<u64>(),
                            },
                            factory_info: StakeInfo {
                                stake: vr.factoryInfo.stake,
                                unstake_delay_sec: vr.factoryInfo.unstakeDelaySec.to::<u64>(),
                            },
                            paymaster_info: StakeInfo {
                                stake: vr.paymasterInfo.stake,
                                unstake_delay_sec: vr.paymasterInfo.unstakeDelaySec.to::<u64>(),
                            },
                            aggregator: if vr.aggregatorInfo.aggregator == Address::ZERO {
                                None
                            } else {
                                Some(vr.aggregatorInfo.aggregator)
                            },
                        }))
                    }
                    Err(e) => Err(ProviderError::InvalidResponse(format!(
                        "failed to decode simulateValidation result: {e}"
                    ))),
                }
            }
            Err(e) => {
                let revert_data = extract_revert_data(&e.to_string());
                match revert_data {
                    Some(data) => Ok(Err(decode_validation_revert(&data))),
                    None => Err(ProviderError::SimulationFailed(e.to_string())),
                }
            }
        }
    }

    async fn simulate_handle_op(
        &self,
        user_op: PackedUserOperation,
        target: Address,
        target_call_data: Bytes,
    ) -> ProviderResult<Result<ExecutionResult, ValidationRevert>> {
        let packed = convert_packed_uo(&user_op);
        let calldata = IEntryPointSimulations::simulateHandleOpCall {
            op: packed,
            target,
            targetCallData: target_call_data,
        }
        .abi_encode();

        let tx = alloy_rpc_types_eth::TransactionRequest::default()
            .to(self.entry_point)
            .input(Bytes::from(calldata).into());

        let res = self.provider.call(tx).await;

        match res {
            Ok(output) => {
                match IEntryPointSimulations::simulateHandleOpCall::abi_decode_returns(&output) {
                    Ok(er) => Ok(Ok(ExecutionResult {
                        pre_op_gas: er.preOpGas.to::<u128>(),
                        paid: er.paid,
                        target_success: er.targetSuccess,
                        target_result: er.targetResult,
                    })),
                    Err(e) => Err(ProviderError::InvalidResponse(format!(
                        "failed to decode simulateHandleOp result: {e}"
                    ))),
                }
            }
            Err(e) => {
                let revert_data = extract_revert_data(&e.to_string());
                match revert_data {
                    Some(data) => Ok(Err(decode_validation_revert(&data))),
                    None => Err(ProviderError::SimulationFailed(e.to_string())),
                }
            }
        }
    }

    fn encode_handle_ops(&self, ops: Vec<PackedUserOperation>, beneficiary: Address) -> Bytes {
        let packed_ops: Vec<_> = ops.iter().map(convert_packed_uo).collect();
        let calldata = IEntryPoint::handleOpsCall {
            ops: packed_ops,
            beneficiary,
        }
        .abi_encode();
        Bytes::from(calldata)
    }

    async fn get_user_op_hash(&self, user_op: PackedUserOperation) -> ProviderResult<B256> {
        let packed = convert_packed_uo(&user_op);
        let calldata = IEntryPoint::getUserOpHashCall { userOp: packed }.abi_encode();
        let tx = alloy_rpc_types_eth::TransactionRequest::default()
            .to(self.entry_point)
            .input(Bytes::from(calldata).into());

        let res = self
            .provider
            .call(tx)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        let hash = IEntryPoint::getUserOpHashCall::abi_decode_returns(&res).map_err(|e| {
            ProviderError::InvalidResponse(format!("failed to decode getUserOpHash: {e}"))
        })?;

        Ok(hash)
    }

    async fn get_balance_of(&self, address: Address) -> ProviderResult<U256> {
        let calldata = IEntryPoint::balanceOfCall { account: address }.abi_encode();

        let tx = alloy_rpc_types_eth::TransactionRequest::default()
            .to(self.entry_point)
            .input(Bytes::from(calldata).into());

        let output = self
            .provider
            .call(tx)
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        let balance = IEntryPoint::balanceOfCall::abi_decode_returns(&output).map_err(|e| {
            ProviderError::InvalidResponse(format!("failed to decode balanceOf: {e}"))
        })?;

        Ok(balance)
    }
}

fn convert_packed_uo(uo: &PackedUserOperation) -> crate::contracts::v0_7::PackedUserOperation {
    crate::contracts::v0_7::PackedUserOperation {
        sender: uo.sender,
        nonce: uo.nonce,
        initCode: uo.initCode.clone(),
        callData: uo.callData.clone(),
        accountGasLimits: uo.accountGasLimits,
        preVerificationGas: uo.preVerificationGas,
        gasFees: uo.gasFees,
        paymasterAndData: uo.paymasterAndData.clone(),
        signature: uo.signature.clone(),
    }
}

/// Try to extract raw revert data from an error string
fn extract_revert_data(err_str: &str) -> Option<Vec<u8>> {
    if let Some(pos) = err_str.find("0x") {
        let hex_str = &err_str[pos + 2..];
        let hex_end = hex_str
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(hex_str.len());
        let hex_data = &hex_str[..hex_end];
        if hex_data.len() >= 8 {
            return const_hex::decode(hex_data).ok();
        }
    }
    None
}

/// Decode a validation revert from raw revert data
fn decode_validation_revert(data: &[u8]) -> ValidationRevert {
    if let Ok(err) = IEntryPoint::IEntryPointErrors::abi_decode(data) {
        return match err {
            IEntryPoint::IEntryPointErrors::FailedOp(fo) => ValidationRevert::FailedOp {
                op_index: fo.opIndex.to::<usize>(),
                reason: fo.reason,
            },
            IEntryPoint::IEntryPointErrors::FailedOpWithRevert(fo) => {
                ValidationRevert::FailedOpWithRevert {
                    op_index: fo.opIndex.to::<usize>(),
                    reason: fo.reason,
                    inner: fo.inner.to_vec(),
                }
            }
            IEntryPoint::IEntryPointErrors::SignatureValidationFailed(svf) => {
                ValidationRevert::SignatureValidationFailed {
                    aggregator: svf.aggregator,
                }
            }
        };
    }

    ValidationRevert::Unknown(data.to_vec())
}

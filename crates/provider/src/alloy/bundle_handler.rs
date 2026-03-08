use alloy_primitives::{Address, Bytes};
use alloy_provider::Provider;
use alloy_sol_types::{SolCall, SolInterface};
use alloy_transport::DualTransportHandler;
use async_trait::async_trait;

use bunzo_types::gas::GasFees;
use bunzo_types::user_operation::v0_7::PackedUserOperation;

use crate::contracts::v0_7::IEntryPoint;
use crate::error::{ProviderError, ProviderResult};
use crate::traits::{BundleHandler, HandleOpsOut};

pub struct AlloyBundleHandler<P> {
    provider: P,
    entry_point: Address,
}

impl<P> AlloyBundleHandler<P> {
    pub fn new(provider: P, entry_point: Address) -> Self {
        Self {
            provider,
            entry_point,
        }
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

#[async_trait]
impl<P> BundleHandler for AlloyBundleHandler<P>
where
    P: Provider + Send + Sync + 'static,
{
    async fn call_handle_ops(
        &self,
        ops: Vec<PackedUserOperation>,
        beneficiary: Address,
        gas_limit: u64,
        gas_fees: GasFees,
    ) -> ProviderResult<HandleOpsOut> {
        let tx = self.build_handle_ops_tx(ops, beneficiary, gas_limit, gas_fees);
        let res = self.provider.call(tx).await;

        match res {
            Ok(_) => Ok(HandleOpsOut::Success),
            Err(e) => {
                // Try to decode the revert data
                let err_str = e.to_string();
                if let Some(data) = extract_revert_data(&err_str) {
                    if let Ok(err) = IEntryPoint::IEntryPointErrors::abi_decode(&data) {
                        match err {
                            IEntryPoint::IEntryPointErrors::FailedOp(fo) => {
                                return Ok(HandleOpsOut::FailedOp(
                                    fo.opIndex.to::<usize>(),
                                    fo.reason,
                                ));
                            }
                            IEntryPoint::IEntryPointErrors::FailedOpWithRevert(fo) => {
                                return Ok(HandleOpsOut::FailedOp(
                                    fo.opIndex.to::<usize>(),
                                    fo.reason,
                                ));
                            }
                            IEntryPoint::IEntryPointErrors::SignatureValidationFailed(svf) => {
                                return Ok(HandleOpsOut::SignatureValidationFailed(svf.aggregator));
                            }
                        }
                    }
                    return Ok(HandleOpsOut::Revert(Bytes::from(data)));
                }
                Err(ProviderError::Transport(e.to_string()))
            }
        }
    }

    fn build_handle_ops_tx(
        &self,
        ops: Vec<PackedUserOperation>,
        beneficiary: Address,
        gas_limit: u64,
        gas_fees: GasFees,
    ) -> alloy_rpc_types_eth::TransactionRequest {
        let packed_ops = ops.iter().map(convert_packed_uo).collect();
        let calldata = IEntryPoint::handleOpsCall {
            ops: packed_ops,
            beneficiary,
        }
        .abi_encode();

        alloy_rpc_types_eth::TransactionRequest::default()
            .to(self.entry_point)
            .input(Bytes::from(calldata).into())
            .gas_limit(gas_limit)
            .max_fee_per_gas(gas_fees.max_fee_per_gas)
            .max_priority_fee_per_gas(gas_fees.max_priority_fee_per_gas)
    }
}

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

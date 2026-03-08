use alloy_primitives::{Address, B256};

use bunzo_types::gas::GasFees;
use bunzo_types::user_operation::v0_7::PackedUserOperation;

#[derive(Debug, Clone)]
pub struct Bundle {
    /// The packed user operations included in this bundle
    pub ops: Vec<PackedUserOperation>,
    /// The beneficiary address (receives gas refunds)
    pub beneficiary: Address,
    pub gas_estimate: u64,
    pub gas_fees: GasFees,
    pub rejected_ops: Vec<RejectedOp>,
}

#[derive(Debug, Clone)]
pub struct RejectedOp {
    pub hash: B256,
    pub reason: String,
}

impl Bundle {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }
}

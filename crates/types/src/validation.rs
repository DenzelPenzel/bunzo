use alloy_primitives::{Address, B256, U256};
use std::collections::HashMap;

/// Expected storage values for conditional transaction submission
///
/// Maps slot → expected value for each address. Used with
/// `eth_sendRawTransactionConditional` to ensure bundle validity at inclusion time
pub type ExpectedStorage = HashMap<Address, HashMap<B256, B256>>;

/// Output from entry point simulation validation
#[derive(Debug, Clone, Default)]
pub struct ValidationOutput {
    /// Gas used during the pre-operation (validation) phase
    pub pre_op_gas: u128,
    /// Required prefund amount
    pub prefund: U256,
    /// Earliest timestamp at which this operation is valid
    pub valid_after: u64,
    /// Latest timestamp at which this operation is valid (0 = no expiry)
    pub valid_until: u64,
    /// Whether the account signature validated correctly
    pub account_sig_failed: bool,
    /// Whether the paymaster signature validated correctly
    pub paymaster_sig_failed: bool,
    /// Stake info for the sender
    pub sender_info: StakeInfo,
    /// Stake info for the factory (if present)
    pub factory_info: StakeInfo,
    /// Stake info for the paymaster (if present)
    pub paymaster_info: StakeInfo,
    /// Aggregator address (if used)
    pub aggregator: Option<Address>,
}

/// Staking information for an entity from the entry point
#[derive(Debug, Clone, Copy, Default)]
pub struct StakeInfo {
    /// Amount staked (in wei)
    pub stake: U256,
    /// Delay before unstaking (in seconds)
    pub unstake_delay_sec: u64,
}

impl StakeInfo {
    /// Whether this entity meets the minimum staking requirements
    pub fn is_staked(&self, min_stake: U256, min_unstake_delay: u64) -> bool {
        self.stake >= min_stake && self.unstake_delay_sec >= min_unstake_delay
    }
}

#[derive(Debug, Clone)]
pub enum ValidationRevert {
    /// The entry point reverted with a FailedOp error
    FailedOp { op_index: usize, reason: String },
    /// The entry point reverted with a FailedOpWithRevert error
    FailedOpWithRevert {
        op_index: usize,
        reason: String,
        inner: Vec<u8>,
    },
    /// Signature validation failed for aggregator
    SignatureValidationFailed { aggregator: Address },
    /// An unknown revert
    Unknown(Vec<u8>),
}

impl std::fmt::Display for ValidationRevert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedOp { op_index, reason } => {
                write!(f, "FailedOp(index={op_index}, reason={reason})")
            }
            Self::FailedOpWithRevert {
                op_index,
                reason,
                inner,
            } => {
                write!(
                    f,
                    "FailedOpWithRevert(index={op_index}, reason={reason}, inner=0x{})",
                    const_hex::encode(inner)
                )
            }
            Self::SignatureValidationFailed { aggregator } => {
                write!(f, "SignatureValidationFailed(aggregator={aggregator})")
            }
            Self::Unknown(data) => {
                write!(f, "Unknown(0x{})", const_hex::encode(data))
            }
        }
    }
}

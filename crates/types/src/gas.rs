use serde::{Deserialize, Serialize};

/// EIP-1559 gas fee parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GasFees {
    /// Maximum fee per gas the sender is willing to pay
    pub max_fee_per_gas: u128,
    /// Maximum priority fee (tip) per gas
    pub max_priority_fee_per_gas: u128,
}


impl GasFees {
    pub fn new(max_fee_per_gas: u128, max_priority_fee_per_gas: u128) -> Self {
        Self {
            max_fee_per_gas,
            max_priority_fee_per_gas,
        }
    }

    /// Compute the effective gas price given a base fee
    pub fn effective_gas_price(&self, base_fee: u128) -> u128 {
        std::cmp::min(
            self.max_fee_per_gas,
            base_fee + self.max_priority_fee_per_gas,
        )
    }

    /// Return true if these fees would cover the given base fee and priority fee
    pub fn covers(&self, base_fee: u128, min_priority_fee: u128) -> bool {
        self.max_fee_per_gas >= base_fee + min_priority_fee
            && self.max_priority_fee_per_gas >= min_priority_fee
    }

    /// Increase fees by a percentage (basis points, 10000 = 100%)
    pub fn increase_by_percent(&self, basis_points: u32) -> Self {
        let multiplier = 10000u128 + basis_points as u128;
        Self {
            max_fee_per_gas: self.max_fee_per_gas * multiplier / 10000,
            max_priority_fee_per_gas: self.max_priority_fee_per_gas * multiplier / 10000,
        }
    }
}

/// Gas estimation result for a user operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GasEstimate {
    pub pre_verification_gas: u128,
    pub call_gas_limit: u128,
    pub verification_gas_limit: u128,
    pub paymaster_verification_gas_limit: Option<u128>,
}

impl std::fmt::Display for GasFees {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GasFees {{ max_fee: {}, priority: {} }}",
            self.max_fee_per_gas, self.max_priority_fee_per_gas
        )
    }
}

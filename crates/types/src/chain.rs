use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSpec {
    /// Human-readable chain name
    pub name: String,
    /// Chain ID.
    pub id: u64,
    /// EntryPoint v0.7 contract address
    pub entry_point_v0_7: Address,
    /// Whether EIP-1559 is enabled
    pub eip1559_enabled: bool,

    // -- Gas parameters --
    /// Minimum gas required for deposit transfer overhead
    pub deposit_transfer_overhead: u64,
    /// Maximum transaction size in bytes
    pub max_transaction_size_bytes: usize,
    /// Block gas limit
    pub block_gas_limit: u64,
    /// Transaction intrinsic gas cost
    pub transaction_intrinsic_gas: u64,
    /// Per-user-op gas overhead (v0.7)
    pub per_user_op_v0_7_gas: u64,
    /// Per-user-op deploy overhead gas
    pub per_user_op_deploy_overhead_gas: u64,
    /// Gas cost per calldata word (32 bytes)
    pub per_user_op_word_gas: u64,
    /// Gas cost for a zero byte of calldata
    pub calldata_zero_byte_gas: u64,
    /// Gas cost for a non-zero byte of calldata
    pub calldata_non_zero_byte_gas: u64,

    // -- Bundle settings --
    /// Maximum interval in milliseconds between bundle send attempts
    pub bundle_max_send_interval_millis: u64,
    /// Number of confirmations required before considering a bundle final
    pub required_confirmations: u64,
    /// Maximum number of operations per bundle
    pub max_bundle_size: usize,

    // -- Pool settings --
    /// Maximum number of operations in the mempool
    pub max_pool_size: usize,
    /// Maximum number of operations per sender
    pub max_ops_per_sender: usize,
    /// Number of blocks of chain history to track for reorg detection
    pub chain_history_size: u64,
    /// Nonce sequence gap limit
    pub nonce_gap_limit: u64,

    // -- Fee settings --
    /// Minimum max priority fee per gas (in wei)
    pub min_max_priority_fee_per_gas: u128,
    /// Maximum max priority fee per gas (in wei)
    pub max_max_priority_fee_per_gas: u128,
    /// Minimum fee bump percentage required for replacement (basis points)
    pub replacement_fee_bump_bps: u32,
}

impl ChainSpec {
    pub fn entry_points(&self) -> Vec<Address> {
        vec![self.entry_point_v0_7]
    }

    pub fn is_entry_point(&self, address: &Address) -> bool {
        *address == self.entry_point_v0_7
    }

    pub fn mainnet() -> Self {
        Self {
            name: "mainnet".to_string(),
            id: 1,
            // ERC-4337 v0.7 EntryPoint canonical address
            entry_point_v0_7: "0x0000000071727De22E5E9d8BAf0edAc6f37da032"
                .parse()
                .unwrap(),
            eip1559_enabled: true,
            deposit_transfer_overhead: 30_000,
            max_transaction_size_bytes: 131_072,
            block_gas_limit: 30_000_000,
            transaction_intrinsic_gas: 21_000,
            per_user_op_v0_7_gas: 19_500,
            per_user_op_deploy_overhead_gas: 10_000,
            per_user_op_word_gas: 4,
            calldata_zero_byte_gas: 4,
            calldata_non_zero_byte_gas: 16,
            bundle_max_send_interval_millis: 1_000,
            required_confirmations: 3,
            max_bundle_size: 16,
            max_pool_size: 4096,
            max_ops_per_sender: 4,
            chain_history_size: 128,
            nonce_gap_limit: 20,
            min_max_priority_fee_per_gas: 100_000_000, // 0.1 gwei
            max_max_priority_fee_per_gas: 50_000_000_000_000, // 50k gwei
            replacement_fee_bump_bps: 1000,            // 10%
        }
    }

    pub fn dev() -> Self {
        Self {
            name: "dev".to_string(),
            id: 31337,
            entry_point_v0_7: Address::ZERO,
            eip1559_enabled: true,
            deposit_transfer_overhead: 30_000,
            max_transaction_size_bytes: 131_072,
            block_gas_limit: 30_000_000,
            transaction_intrinsic_gas: 21_000,
            per_user_op_v0_7_gas: 19_500,
            per_user_op_deploy_overhead_gas: 10_000,
            per_user_op_word_gas: 4,
            calldata_zero_byte_gas: 4,
            calldata_non_zero_byte_gas: 16,
            bundle_max_send_interval_millis: 100,
            required_confirmations: 1,
            max_bundle_size: 16,
            max_pool_size: 4096,
            max_ops_per_sender: 4,
            chain_history_size: 64,
            nonce_gap_limit: 20,
            min_max_priority_fee_per_gas: 0,
            max_max_priority_fee_per_gas: 50_000_000_000_000,
            replacement_fee_bump_bps: 1000,
        }
    }
}

impl Default for ChainSpec {
    fn default() -> Self {
        Self::mainnet()
    }
}

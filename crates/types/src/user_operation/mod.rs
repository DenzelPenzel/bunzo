pub mod v0_7;

use alloy_primitives::{Address, Bytes, B256, U256};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::entity::{Entity, EntityType};
use crate::gas::GasFees;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntryPointVersion {
    V0_7,
}

impl fmt::Display for EntryPointVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V0_7 => write!(f, "v0.7"),
        }
    }
}

/// Unique identifier for a user operation: (sender, nonce)
///
/// This is the correct key for replacement logic. When a new UO arrives with the same
/// (sender, nonce) pair, it replaces the existing one. Using hash-based lookup for
/// replacement is buggy — if UO1 mines after UO2 replaced it, the pool must remove UO2
/// by its ID, not by UO1's hash
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserOperationId {
    pub sender: Address,
    pub nonce: U256,
}

impl UserOperationId {
    pub fn new(sender: Address, nonce: U256) -> Self {
        Self { sender, nonce }
    }

     /// Extract the nonce key (upper 192 bits) from the 2D nonce
    pub fn nonce_key(&self) -> U256 {
        self.nonce >> 64
    }

    /// Extract the nonce sequence (lower 64 bits) from the 2D nonce
    pub fn nonce_sequence(&self) -> u64 {
        let mask = U256::from(u64::MAX);
        (self.nonce & mask).to::<u64>()
    }
}

impl fmt::Display for UserOperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.sender, self.nonce)
    }
}

pub trait UserOperation: Send + Sync + Clone + fmt::Debug {
    fn entry_point_version(&self) -> EntryPointVersion;

    /// The account that originates the operation
    fn sender(&self) -> Address;

     /// The operation nonce
    fn nonce(&self) -> U256;

    /// The call data to execute on the sender account
    fn call_data(&self) -> &Bytes;
    
    /// The operation signature
    fn signature(&self) -> &Bytes;

    /// The operation hash (unique across chain + entry point + content)
    fn hash(&self) -> B256;

    /// The unique ID for replacement purposes
    fn id(&self) -> UserOperationId {
        UserOperationId::new(self.sender(), self.nonce())
    }

    fn factory(&self) -> Option<Address>;
    fn paymaster(&self) -> Option<Address>;

    fn call_gas_limit(&self) -> u128;
    fn verification_gas_limit(&self) -> u128;
    fn pre_verification_gas(&self) -> u128;
    fn max_fee_per_gas(&self) -> u128;
    fn max_priority_fee_per_gas(&self) -> u128;


    /// Paymaster verification gas limit (0 if no paymaster)
    fn paymaster_verification_gas_limit(&self) -> u128;

    /// Paymaster post-op gas limit (0 if no paymaster)
    fn paymaster_post_op_gas_limit(&self) -> u128;

    /// Gas fees for this operation
    fn gas_fees(&self) -> GasFees {
        GasFees::new(self.max_fee_per_gas(), self.max_priority_fee_per_gas())
    }

    /// Effective gas price given a base fee
    fn effective_gas_price(&self, base_fee: u128) -> u128 {
        self.gas_fees().effective_gas_price(base_fee)
    }

    /// Total gas limit for this operation (all phases combined)
    fn total_gas_limit(&self) -> u128 {
        self.pre_verification_gas()
            + self.verification_gas_limit()
            + self.call_gas_limit()
            + self.paymaster_verification_gas_limit()
            + self.paymaster_post_op_gas_limit()
    }

    /// Maximum cost this operation could incur
    fn max_gas_cost(&self) -> u128 {
        self.total_gas_limit() * self.max_fee_per_gas()
    }

    /// All entities referenced by this operation
    fn entities(&self) -> Vec<Entity> {
        let mut entities = vec![Entity::account(self.sender())];
        if let Some(factory) = self.factory() {
            entities.push(Entity::factory(factory));
        }
        if let Some(paymaster) = self.paymaster() {
            entities.push(Entity::paymaster(paymaster));
        }
        entities
    }

    /// The entity at a given type, if present
    fn entity(&self, kind: EntityType) -> Option<Entity> {
        match kind {
            EntityType::Account => Some(Entity::account(self.sender())),
            EntityType::Factory => self.factory().map(Entity::factory),
            EntityType::Paymaster => self.paymaster().map(Entity::paymaster),
            EntityType::Aggregator => None,
        }
    }

    /// Size of this operation in bytes when ABI-encoded for submission
    fn abi_encoded_size(&self) -> usize;

    /// Compute the calldata gas cost for this operation
    fn calldata_gas_cost(&self, zero_byte_cost: u64, non_zero_byte_cost: u64) -> u128;
}


/// Variant enum wrapping all supported user operation versions
#[derive(Debug, Clone)]
pub enum UserOperationVariant {
    V0_7(v0_7::UserOperation),
}

impl UserOperationVariant {
    pub fn entry_point_version(&self) -> EntryPointVersion {
        match self {
            Self::V0_7(_) => EntryPointVersion::V0_7,
        }
    }

    pub fn hash(&self) -> B256 {
        match self {
            Self::V0_7(uo) => uo.hash(),
        }
    }

    pub fn sender(&self) -> Address {
        match self {
            Self::V0_7(uo) => uo.sender(),
        }
    }

    pub fn nonce(&self) -> U256 {
        match self {
            Self::V0_7(uo) => uo.nonce(),
        }
    }

    pub fn id(&self) -> UserOperationId {
        match self {
            Self::V0_7(uo) => uo.id(),
        }
    }

    pub fn max_fee_per_gas(&self) -> u128 {
        match self {
            Self::V0_7(uo) => uo.max_fee_per_gas(),
        }
    }

    pub fn max_priority_fee_per_gas(&self) -> u128 {
        match self {
            Self::V0_7(uo) => uo.max_priority_fee_per_gas(),
        }
    }
}

impl From<v0_7::UserOperation> for UserOperationVariant {
    fn from(uo: v0_7::UserOperation) -> Self {
        Self::V0_7(uo)
    }
}
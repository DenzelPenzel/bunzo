use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityType {
    Account,
    Paymaster,
    Factory,
    Aggregator,
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Account => write!(f, "account"),
            Self::Paymaster => write!(f, "paymaster"),
            Self::Factory => write!(f, "factory"),
            Self::Aggregator => write!(f, "aggregator"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    pub kind: EntityType,
    pub address: Address,
}

impl Entity {
    pub fn new(kind: EntityType, address: Address) -> Self {
        Self { kind, address }
    }

    pub fn account(address: Address) -> Self {
        Self::new(EntityType::Account, address)
    }

    pub fn paymaster(address: Address) -> Self {
        Self::new(EntityType::Paymaster, address)
    }

    pub fn factory(address: Address) -> Self {
        Self::new(EntityType::Factory, address)
    }

    pub fn aggregator(address: Address) -> Self {
        Self::new(EntityType::Aggregator, address)
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.address)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityInfo {
    pub entity: Entity,
    pub is_staked: bool,
}

#[derive(Debug, Clone)]
pub struct EntityInfos {
    pub sender: EntityInfo,
    pub factory: Option<EntityInfo>,
    pub paymaster: Option<EntityInfo>,
    pub aggregator: Option<EntityInfo>,
}

impl EntityInfos {
    pub fn entities(&self) -> impl Iterator<Item = &EntityInfo> {
        std::iter::once(&self.sender)
            .chain(self.factory.iter())
            .chain(self.paymaster.iter())
            .chain(self.aggregator.iter())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityUpdateType {
    /// Entity was involved in an unstaked invalidation
    UnstakedInvalidation,
    /// Entity was involved in a staked invalidation
    StakedInvalidation,
}

#[derive(Debug, Clone)]
pub struct EntityUpdate {
    pub entity: Entity,
    pub update_type: EntityUpdateType,
}

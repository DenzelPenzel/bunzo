use std::collections::HashMap;

use alloy_primitives::Address;
use parking_lot::RwLock;
use tracing::debug;

use bunzo_types::entity::Entity;

/// ERC-7562 reputation status for an entity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReputationStatus {
    /// Entity is in good standing
    Ok,
    /// Entity is being throttled (limited operations allowed)
    Throttled,
    /// Entity is banned (no operations allowed)
    Banned,
}

/// Tracked reputation data for a single entity
#[derive(Debug, Clone)]
struct ReputationEntry {
    /// Number of operations seen (submitted) by this entity
    ops_seen: u64,
    /// Number of operations included (mined) for this entity
    ops_included: u64,
}

impl ReputationEntry {
    fn new() -> Self {
        Self {
            ops_seen: 0,
            ops_included: 0,
        }
    }

    /// Compute reputation status based on inclusion ratio
    fn status(&self, config: &ReputationConfig) -> ReputationStatus {
        if self.ops_seen == 0 {
            return ReputationStatus::Ok;
        }

        if self.ops_seen > config.min_ops_for_throttle {
            let ratio = self.ops_included as f64 / self.ops_seen as f64;
            if ratio < config.ban_threshold {
                return ReputationStatus::Banned;
            }
            if ratio < config.throttle_threshold {
                return ReputationStatus::Throttled;
            }
        }

        ReputationStatus::Ok
    }
}

/// Configuration for the reputation manager
#[derive(Debug, Clone)]
pub struct ReputationConfig {
    /// Minimum number of operations seen before throttling kicks in
    pub min_ops_for_throttle: u64,
    /// Inclusion ratio below which an entity is throttled
    pub throttle_threshold: f64,
    /// Inclusion ratio below which an entity is banned
    pub ban_threshold: f64,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            min_ops_for_throttle: 10,
            throttle_threshold: 0.1,
            ban_threshold: 0.01,
        }
    }
}

/// ERC-7562 entity reputation tracker
pub struct ReputationManager {
    entries: RwLock<HashMap<Address, ReputationEntry>>,
    config: ReputationConfig,
}

impl ReputationManager {
    pub fn new(config: ReputationConfig) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            config,
        }
    }

    pub fn status(&self, address: &Address) -> ReputationStatus {
        self.entries
            .read()
            .get(address)
            .map(|e| e.status(&self.config))
            .unwrap_or(ReputationStatus::Ok)
    }

    pub fn check_entity(&self, entity: &Entity) -> Result<(), ReputationStatus> {
        let status = self.status(&entity.address);
        match status {
            ReputationStatus::Ok => Ok(()),
            status => {
                debug!(
                    entity = %entity,
                    status = ?status,
                    "entity reputation check failed"
                );
                Err(status)
            }
        }
    }

    /// Record that an operation was seen (submitted) involving this entity
    pub fn record_seen(&self, address: &Address) {
        self.entries
            .write()
            .entry(*address)
            .or_insert_with(ReputationEntry::new)
            .ops_seen += 1;
    }

    /// Record that an operation was included (mined) for this entity
    pub fn record_included(&self, address: &Address) {
        self.entries
            .write()
            .entry(*address)
            .or_insert_with(ReputationEntry::new)
            .ops_included += 1;
    }

    pub fn hourly_decay(&self) {
        let mut entries = self.entries.write();
        entries.retain(|_, entry| {
            // Decay by dividing both counters (integer division rounds down)
            entry.ops_seen = entry.ops_seen * 23 / 24;
            entry.ops_included = entry.ops_included * 23 / 24;
            entry.ops_seen > 0 || entry.ops_included > 0
        });
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn dump(&self) -> Vec<(Address, ReputationStatus)> {
        self.entries
            .read()
            .iter()
            .map(|(addr, entry)| (*addr, entry.status(&self.config)))
            .collect()
    }
}

impl Default for ReputationManager {
    fn default() -> Self {
        Self::new(ReputationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_entity_is_ok() {
        let mgr = ReputationManager::default();
        let addr = Address::repeat_byte(0x01);
        assert_eq!(mgr.status(&addr), ReputationStatus::Ok);
    }

    #[test]
    fn test_throttle() {
        let mgr = ReputationManager::new(ReputationConfig {
            min_ops_for_throttle: 5,
            throttle_threshold: 0.5,
            ban_threshold: 0.01,
        });
        let addr = Address::repeat_byte(0x01);

        // 10 seen, 1 included → ratio 0.1 < 0.5 → throttled
        for _ in 0..10 {
            mgr.record_seen(&addr);
        }
        mgr.record_included(&addr);

        assert_eq!(mgr.status(&addr), ReputationStatus::Throttled);
    }

    #[test]
    fn test_good_reputation() {
        let mgr = ReputationManager::default();
        let addr = Address::repeat_byte(0x01);

        for _ in 0..20 {
            mgr.record_seen(&addr);
            mgr.record_included(&addr);
        }

        assert_eq!(mgr.status(&addr), ReputationStatus::Ok);
    }
}

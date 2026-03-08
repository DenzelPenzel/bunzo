use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use alloy_primitives::{Address, B256, U256};
use parking_lot::RwLock;
use tracing::{debug, info};

use bunzo_types::chain::ChainSpec;
use bunzo_types::user_operation::v0_7::UserOperation;
use bunzo_types::user_operation::{UserOperation as UserOperationTrait, UserOperationId};

use crate::error::PoolError;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_size: usize,
    pub max_ops_per_sender: usize,
    /// Minimum fee bump percentage required for replacement (basis points, 10000 = 100%)
    pub replacement_fee_bump_bps: u32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 4096,
            max_ops_per_sender: 4,
            replacement_fee_bump_bps: 1000,
        }
    }
}

impl From<&ChainSpec> for PoolConfig {
    fn from(chain_spec: &ChainSpec) -> Self {
        Self {
            max_size: chain_spec.max_pool_size,
            max_ops_per_sender: chain_spec.max_ops_per_sender,
            replacement_fee_bump_bps: chain_spec.replacement_fee_bump_bps,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolEntry {
    pub uo: UserOperation,
    pub hash: B256,
    pub id: UserOperationId,
    /// Effective gas price at submission time (for ordering)
    pub submit_gas_price: u128,
    /// When this entry was added to the pool (monotonic counter)
    pub sequence: u64,
}

impl PoolEntry {
    pub fn new(uo: UserOperation, base_fee: u128, sequence: u64) -> Self {
        let hash = uo.hash();
        let id = uo.id();
        let submit_gas_price = uo.effective_gas_price(base_fee);
        Self {
            uo,
            hash,
            id,
            submit_gas_price,
            sequence,
        }
    }
}

/// Ordering wrapper for BTreeSet:
/// highest gas price first, then lowest sequence (FIFO tiebreak)
#[derive(Debug, Clone)]
struct OrderEntry(Arc<PoolEntry>);

impl PartialEq for OrderEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.hash == other.0.hash
    }
}

impl Eq for OrderEntry {}

impl PartialOrd for OrderEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .submit_gas_price
            .cmp(&self.0.submit_gas_price)
            .then_with(|| self.0.sequence.cmp(&other.0.sequence))
            .then_with(|| self.0.hash.cmp(&other.0.hash))
    }
}

pub struct OperationPool {
    /// Index by operation hash
    by_hash: RwLock<HashMap<B256, Arc<PoolEntry>>>,
    /// Index by operation ID (sender + nonce)
    by_id: RwLock<HashMap<UserOperationId, Arc<PoolEntry>>>,
    ordered: RwLock<BTreeSet<OrderEntry>>,
    sender_count: RwLock<HashMap<Address, usize>>,
    /// Monotonically increasing sequence counter
    next_sequence: RwLock<u64>,
    config: PoolConfig,
}

impl OperationPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            by_hash: RwLock::new(HashMap::new()),
            by_id: RwLock::new(HashMap::new()),
            ordered: RwLock::new(BTreeSet::new()),
            sender_count: RwLock::new(HashMap::new()),
            next_sequence: RwLock::new(0),
            config,
        }
    }

    /// Return the number of operations in the pool
    pub fn size(&self) -> usize {
        self.by_hash.read().len()
    }

    pub fn add(&self, uo: UserOperation, base_fee: u128) -> Result<B256, PoolError> {
        let id = uo.id();
        let hash = uo.hash();

        if self.by_hash.read().contains_key(&hash) {
            return Err(PoolError::AlreadyKnown(hash));
        }

        let existing_entry = self.by_id.read().get(&id).cloned();
        if let Some(entry) = existing_entry {
            let existing_fee = entry.uo.max_fee_per_gas();
            let new_fee = uo.max_fee_per_gas();
            let min_bump =
                existing_fee + existing_fee * self.config.replacement_fee_bump_bps as u128 / 10000;

            if new_fee < min_bump {
                return Err(PoolError::ReplacementUnderpriced {
                    existing: existing_fee,
                    new: new_fee,
                });
            }

            let existing_hash = entry.hash;
            self.remove_inner(&entry);
            debug!(
                old_hash = %existing_hash,
                new_hash = %hash,
                sender = %id.sender,
                "replaced user operation"
            );
        }

        let sender_count = self
            .sender_count
            .read()
            .get(&id.sender)
            .copied()
            .unwrap_or(0);
        if sender_count >= self.config.max_ops_per_sender {
            return Err(PoolError::Other(format!(
                "sender {} already has {} operations (max {})",
                id.sender, sender_count, self.config.max_ops_per_sender
            )));
        }

        if self.size() >= self.config.max_size {
            return Err(PoolError::PoolFull {
                max_size: self.config.max_size,
            });
        }

        let sequence = {
            let mut seq = self.next_sequence.write();
            let s = *seq;
            *seq = seq.saturating_add(1);
            s
        };

        let entry = Arc::new(PoolEntry::new(uo, base_fee, sequence));

        self.by_hash.write().insert(hash, entry.clone());
        self.by_id.write().insert(id, entry.clone());
        self.ordered.write().insert(OrderEntry(entry.clone()));
        *self.sender_count.write().entry(id.sender).or_insert(0) += 1;

        let pool_size = self.size();

        info!(
            hash = %hash,
            sender = %id.sender,
            nonce = %id.nonce,
            gas_price = entry.submit_gas_price,
            pool_size,
            "added user operation to pool"
        );

        Ok(hash)
    }

    pub fn get_by_hash(&self, hash: &B256) -> Option<Arc<PoolEntry>> {
        self.by_hash.read().get(hash).cloned()
    }

    pub fn get_by_id(&self, id: &UserOperationId) -> Option<Arc<PoolEntry>> {
        self.by_id.read().get(id).cloned()
    }

    pub fn remove_by_hash(&self, hash: &B256) -> Option<Arc<PoolEntry>>{
        let entry = self.by_hash.read().get(hash).cloned()?;
        self.remove_inner(&entry);
        Some(entry)
    }

    pub fn remove_by_id(&self, id: &UserOperationId) -> Option<Arc<PoolEntry>> {
        let entry = self.by_id.read().get(id).cloned()?;
        self.remove_inner(&entry);
        Some(entry)
    }

    pub fn best_operations(&self, max_count: usize) -> Vec<Arc<PoolEntry>> {
        self.ordered
            .read()
            .iter()
            .take(max_count)
            .map(|e| e.0.clone())
            .collect()
    }

    pub fn clear(&self) {
        self.by_hash.write().clear();
        self.by_id.write().clear();
        self.ordered.write().clear();
        self.sender_count.write().clear();
        info!("cleared operation pool");
    }

    pub fn all_hashes(&self) -> Vec<B256> {
        self.by_hash.read().keys().cloned().collect()
    }

    pub fn dump(&self) -> Vec<Arc<PoolEntry>> {
        self.by_hash.read().values().cloned().collect()
    }

    pub fn max_nonce_sequence(&self, sender: &Address, nonce_key: &U256) -> Option<u64> {
        let by_id = self.by_id.read();
        by_id
            .iter()
            .filter(|(id, _)| id.sender == *sender && id.nonce_key() == *nonce_key)
            .map(|(id, _)| id.nonce_sequence())
            .max()
    }

    fn remove_inner(&self, entry: &Arc<PoolEntry>) {
        self.by_hash.write().remove(&entry.hash);
        self.by_id.write().remove(&entry.id);
        self.ordered.write().remove(&OrderEntry(entry.clone()));

        let mut counts = self.sender_count.write();
        if let Some(count) = counts.get_mut(&entry.id.sender) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(&entry.id.sender);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Bytes, U256};

    fn make_uo(sender_byte: u8, nonce: u64, max_fee: u128) -> UserOperation {
        UserOperation::new(
            Address::repeat_byte(sender_byte),
            U256::from(nonce),
            Bytes::from(vec![0xab]),
            100_000,
            200_000,
            50_000,
            max_fee,
            1_000_000_000,
            Bytes::from(vec![0x01]),
            None,
            Bytes::new(),
            None,
            0,
            0,
            Bytes::new(),
            Address::repeat_byte(0xEE),
            1,
        )
    }

    #[test]
    fn test_add_and_get() {
        let pool = OperationPool::new(PoolConfig::default());
        let uo = make_uo(0x01, 0, 30_000_000_000);
        let hash = pool.add(uo.clone(), 15_000_000_000).unwrap();
        assert_eq!(pool.size(), 1);
        assert!(pool.get_by_hash(&hash).is_some());
        assert!(pool.get_by_id(&uo.id()).is_some());
    }

    #[test]
    fn test_duplicate_rejected() {
        let pool = OperationPool::new(PoolConfig::default());
        let uo = make_uo(0x01, 0, 30_000_000_000);
        pool.add(uo.clone(), 15_000_000_000).unwrap();

        let res = pool.add(uo, 15_000_000_000);
        assert!(matches!(res, Err(PoolError::AlreadyKnown(_))));
    }

    #[test]
    fn test_replacement() {
        let pool = OperationPool::new(PoolConfig::default());
        let uo1 = make_uo(0x01, 0, 30_000_000_000);
        let hash1 = pool.add(uo1, 15_000_000_000).unwrap();

        let uo2 = make_uo(0x01, 0, 40_000_000_000);
        let hash2 = pool.add(uo2, 15_000_000_000).unwrap();

        assert_eq!(pool.size(), 1);
        assert!(pool.get_by_hash(&hash1).is_none());
        assert!(pool.get_by_hash(&hash2).is_some());
    }

    #[test]
    fn test_replacement_underpriced() {
        let pool = OperationPool::new(PoolConfig::default());
        let uo1 = make_uo(0x01, 0, 30_000_000_000);
        pool.add(uo1, 15_000_000_000).unwrap();

        let uo2 = make_uo(0x01, 0, 31_000_000_000);
        let result = pool.add(uo2, 15_000_000_000);
        assert!(matches!(
            result,
            Err(PoolError::ReplacementUnderpriced { .. })
        ));
    }

    #[test]
    fn test_remove_by_id_fixes_replacement_bug() {
        let pool = OperationPool::new(PoolConfig::default());

        // Add UO1 with nonce 0
        let uo1 = make_uo(0x01, 0, 30_000_000_000);
        let id = uo1.id();
        let _hash1 = pool.add(uo1, 15_000_000_000).unwrap();

        // Replace with UO2 (same sender + nonce, higher fee)
        let uo2 = make_uo(0x01, 0, 40_000_000_000);
        let hash2 = pool.add(uo2, 15_000_000_000).unwrap();

        // Now simulate mining: remove by ID (not by hash1)
        // This correctly removes UO2, which is the current entry
        let removed = pool.remove_by_id(&id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().hash, hash2);
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_best_operations_ordered() {
        let pool = OperationPool::new(PoolConfig::default());

        pool.add(make_uo(0x01, 0, 10_000_000_000), 5_000_000_000)
            .unwrap();
        pool.add(make_uo(0x02, 0, 30_000_000_000), 5_000_000_000)
            .unwrap();
        pool.add(make_uo(0x03, 0, 20_000_000_000), 5_000_000_000)
            .unwrap();

        let best = pool.best_operations(3);
        assert_eq!(best.len(), 3);

        assert!(best[0].submit_gas_price >= best[1].submit_gas_price);
        assert!(best[1].submit_gas_price >= best[2].submit_gas_price);
    }

    #[test]
    fn test_pool_full() {
        let config = PoolConfig {
            max_size: 2,
            ..PoolConfig::default()
        };
        let pool = OperationPool::new(config);

        pool.add(make_uo(0x01, 0, 30_000_000_000), 15_000_000_000)
            .unwrap();
        pool.add(make_uo(0x02, 0, 30_000_000_000), 15_000_000_000)
            .unwrap();

        let result = pool.add(make_uo(0x03, 0, 30_000_000_000), 15_000_000_000);
        assert!(matches!(result, Err(PoolError::PoolFull { .. })));
    }
}

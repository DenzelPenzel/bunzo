use std::collections::{HashMap, HashSet};

use alloy_primitives::{Address, B256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    Read,
    Write,
}

#[derive(Debug, Clone, Default)]
pub struct StorageAccess {
    slots: HashMap<(Address, B256), AccessType>,
}

impl StorageAccess {
    pub fn record_read(&mut self, address: Address, slot: B256) {
        self.slots
            .entry((address, slot))
            .or_insert(AccessType::Read);
    }

    pub fn record_write(&mut self, address: Address, slot: B256) {
        self.slots.insert((address, slot), AccessType::Write);
    }

    pub fn accessed_slots(&self) -> impl Iterator<Item = (&(Address, B256), &AccessType)> {
        self.slots.iter()
    }

    pub fn written_slots(&self) -> impl Iterator<Item = &(Address, B256)> {
        self.slots
            .iter()
            .filter(|(_, access)| **access == AccessType::Write)
            .map(|(slot, _)| slot)
    }
}

pub struct ConflictDetector {
    accessed: HashMap<(Address, B256), Vec<usize>>,
    written: HashMap<(Address, B256), usize>,
    senders: Vec<Address>,
}

impl ConflictDetector {
    pub fn new() -> Self {
        Self {
            accessed: HashMap::new(),
            written: HashMap::new(),
            senders: Vec::new(),
        }
    }

    pub fn check_conflicts(&self, sender: Address, access: &StorageAccess) -> HashSet<usize> {
        let mut conflicts = HashSet::new();

        for ((addr, slot), access_type) in access.accessed_slots() {
            let key = (*addr, *slot);

            match access_type {
                AccessType::Write => {
                    // Write conflicts with any existing access (read or write)
                    // from a different sender
                    if let Some(indices) = self.accessed.get(&key) {
                        for &idx in indices {
                            if self.senders[idx] != sender {
                                conflicts.insert(idx);
                            }
                        }
                    }
                }
                AccessType::Read => {
                    if let Some(&writer_idx) = self.written.get(&key) {
                        if self.senders[writer_idx] != sender {
                            conflicts.insert(writer_idx);
                        }
                    }
                }
            }
        }

        conflicts
    }

    pub fn add_operation(&mut self, sender: Address, access: &StorageAccess) -> usize {
        let idx = self.senders.len();
        self.senders.push(sender);

        for ((addr, slot), access_type) in access.accessed_slots() {
            let key = (*addr, *slot);
            self.accessed.entry(key).or_default().push(idx);

            if *access_type == AccessType::Write {
                self.written.insert(key, idx);
            }
        }

        idx
    }

    pub fn reset(&mut self) {
        self.accessed.clear();
        self.written.clear();
        self.senders.clear();
    }
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_conflict_independent_ops() {
        let mut detector = ConflictDetector::new();
        let sender_a = Address::repeat_byte(0x01);
        let sender_b = Address::repeat_byte(0x02);
        let slot_1 = B256::repeat_byte(0x01);
        let slot_2 = B256::repeat_byte(0x02);
        let contract = Address::repeat_byte(0xCC);

        // Op A writes to slot 1
        let mut access_a = StorageAccess::default();
        access_a.record_write(contract, slot_1);
        detector.add_operation(sender_a, &access_a);

        // Op B writes to slot 2 = no conflict
        let mut access_b = StorageAccess::default();
        access_b.record_write(contract, slot_2);
        let conflicts = detector.check_conflicts(sender_b, &access_b);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_write_write_conflict() {
        let mut detector = ConflictDetector::new();
        let sender_a = Address::repeat_byte(0x01);
        let sender_b = Address::repeat_byte(0x02);
        let slot = B256::repeat_byte(0x01);
        let contract = Address::repeat_byte(0xCC);

        // Op A writes to slot
        let mut access_a = StorageAccess::default();
        access_a.record_write(contract, slot);
        detector.add_operation(sender_a, &access_a);

        // Op B also writes to same slot = conflict
        let mut access_b = StorageAccess::default();
        access_b.record_write(contract, slot);
        let conflicts = detector.check_conflicts(sender_b, &access_b);
        assert!(conflicts.contains(&0));
    }

    #[test]
    fn test_write_read_conflict() {
        let mut detector = ConflictDetector::new();
        let sender_a = Address::repeat_byte(0x01);
        let sender_b = Address::repeat_byte(0x02);
        let slot = B256::repeat_byte(0x01);
        let contract = Address::repeat_byte(0xCC);

        // Op A writes to slot
        let mut access_a = StorageAccess::default();
        access_a.record_write(contract, slot);
        detector.add_operation(sender_a, &access_a);

        // Op B reads from same slot = conflict
        let mut access_b = StorageAccess::default();
        access_b.record_read(contract, slot);
        let conflicts = detector.check_conflicts(sender_b, &access_b);
        assert!(conflicts.contains(&0));
    }

    #[test]
    fn test_same_sender_no_conflict() {
        let mut detector = ConflictDetector::new();
        let sender = Address::repeat_byte(0x01);
        let slot = B256::repeat_byte(0x01);
        let contract = Address::repeat_byte(0xCC);

        // Op 1 from sender writes to slot
        let mut access_1 = StorageAccess::default();
        access_1.record_write(contract, slot);
        detector.add_operation(sender, &access_1);

        // Op 2 from same sender writes to same slot = no conflict (same sender, ordered by nonce
        let mut access_2 = StorageAccess::default();
        access_2.record_write(contract, slot);
        let conflicts = detector.check_conflicts(sender, &access_2);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_read_read_no_conflict() {
        let mut detector = ConflictDetector::new();
        let sender_a = Address::repeat_byte(0x01);
        let sender_b = Address::repeat_byte(0x02);
        let slot = B256::repeat_byte(0x01);
        let contract = Address::repeat_byte(0xCC);

        // Op A reads from slot
        let mut access_a = StorageAccess::default();
        access_a.record_read(contract, slot);
        detector.add_operation(sender_a, &access_a);

        // Op B also reads from same slot — no conflict (read-read is safe)
        let mut access_b = StorageAccess::default();
        access_b.record_read(contract, slot);
        let conflicts = detector.check_conflicts(sender_b, &access_b);
        assert!(conflicts.is_empty());
    }
}

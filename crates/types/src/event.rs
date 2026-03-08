use alloy_primitives::{Address, B256, U256};

#[derive(Debug, Clone)]
pub enum PoolEvent {
    /// A new user operation was added to the pool
    OpAdded {
        hash: B256,
        sender: Address,
        nonce: U256,
    },
    /// A user operation was removed from the pool (mined, replaced, or evicted)
    OpRemoved { hash: B256, reason: OpRemovalReason },
    /// The pool was cleared
    Cleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpRemovalReason {
    /// The operation was included in a block
    Mined,
    /// The operation was replaced by a higher-fee operation
    Replaced,
    /// The operation was evicted due to pool size limits
    Evicted,
    /// The operation was removed due to a reorg invalidating it
    Reorged,
    /// The operation expired
    Expired,
    /// The operation was manually removed
    Dropped,
}

/// Events emitted by the builder/bundler
#[derive(Debug, Clone)]
pub enum BuilderEvent {
    /// A bundle was proposed
    BundleProposed { ops_count: usize, gas_estimate: u64 },
    /// A bundle transaction was submitted
    BundleSubmitted {
        tx_hash: B256,
        ops_count: usize,
        nonce: u64,
    },
    /// A bundle transaction was mined
    BundleMined {
        tx_hash: B256,
        block_number: u64,
        gas_used: u64,
    },
    /// A bundle transaction was dropped or replaced
    BundleDropped { tx_hash: B256, reason: String },
    /// Fee escalation was triggered for a pending bundle
    FeeEscalated {
        old_tx_hash: B256,
        new_tx_hash: B256,
        attempt: u32,
    },
}

#[derive(Debug, Clone)]
pub enum ChainEvent {
    /// A new block was observed
    NewBlock {
        number: u64,
        hash: B256,
        base_fee: u128,
        timestamp: u64,
    },
    /// A chain reorganization was detected
    Reorg {
        /// The depth of the reorg (number of blocks reverted)
        depth: u64,
        /// The new head block number
        new_head: u64,
    },
}

/// A multi-producer, multi-consumer event bus built
pub struct EventBus<T> {
    sender: tokio::sync::broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    /// Create a new event bus with the given channel capacity
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all subscribers
    ///
    /// Returns the number of receivers that received the event
    /// If there are no active receivers, the event is silently dropped
    pub fn publish(&self, event: T) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribe to events on this bus
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<T> {
        self.sender.subscribe()
    }
}

impl<T: Clone + Send + 'static> Default for EventBus<T> {
    fn default() -> Self {
        Self::new(256)
    }
}

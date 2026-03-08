use std::collections::HashMap;
use std::time::{Duration, Instant};

use alloy_primitives::B256;
use parking_lot::RwLock;

/// The lifecycle status of a user operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserOpStatus {
    /// The operation was received via RPC
    Received,
    /// The operation passed validation
    Validated,
    /// The operation was added to the mempool
    Pooled,
    /// The operation was included in a submitted bundle transaction
    Submitted,
    /// The bundle transaction containing this operation was mined
    Included,
    /// The operation has enough confirmations to be considered final
    Finalized,
    /// The operation failed at some stage
    Failed,
}

impl std::fmt::Display for UserOpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Received => write!(f, "received"),
            Self::Validated => write!(f, "validated"),
            Self::Pooled => write!(f, "pooled"),
            Self::Submitted => write!(f, "submitted"),
            Self::Included => write!(f, "included"),
            Self::Finalized => write!(f, "finalized"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// A status entry with metadata
#[derive(Debug, Clone)]
struct StatusEntry {
    status: UserOpStatus,
    /// When the entry was last updated
    updated_at: Instant,
    detail: Option<String>,
}

/// Tracks the lifecycle status of user operations
pub struct StatusTracker {
    entries: RwLock<HashMap<B256, StatusEntry>>,
    ttl: Duration,
}

impl StatusTracker {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_status(&self, hash: B256, status: UserOpStatus) {
        self.entries.write().insert(
            hash,
            StatusEntry {
                status,
                updated_at: Instant::now(),
                detail: None,
            },
        );
    }

    pub fn set_status_with_detail(&self, hash: B256, status: UserOpStatus, detail: String) {
        self.entries.write().insert(
            hash,
            StatusEntry {
                status,
                updated_at: Instant::now(),
                detail: Some(detail),
            },
        );
    }

    pub fn get_status(&self, hash: &B256) -> Option<UserOpStatus> {
        self.entries.read().get(hash).map(|e| e.status)
    }

    pub fn get_status_detail(&self, hash: &B256) -> Option<(UserOpStatus, Option<String>)> {
        self.entries
            .read()
            .get(hash)
            .map(|e| (e.status, e.detail.clone()))
    }

    pub fn cleanup(&self) {
        let now = Instant::now();
        self.entries
            .write()
            .retain(|_, entry| now.duration_since(entry.updated_at) < self.ttl);
    }

    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }
}

impl Default for StatusTracker {
    fn default() -> Self {
        Self::new(Duration::from_secs(3600))
    }
}

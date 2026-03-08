use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use alloy_primitives::{Address, Bytes};
use alloy_rpc_types_eth::TransactionRequest;
use tracing::debug;

use bunzo_types::error::SignerError;

use crate::BundlerSigner;

/// Multi-wallet signer manager with round-robin allocation
/// Manages a pool of signers (wallets) and distributes signing requests
/// across them. This enables parallel bundle submission from different
/// EOAs, avoiding nonce contention on a single address
pub struct SignerManager {
    signers: Vec<Arc<dyn BundlerSigner>>,
    next_index: AtomicUsize,
}

impl SignerManager {
    pub fn new(signers: Vec<Arc<dyn BundlerSigner>>) -> Self {
        Self {
            signers,
            next_index: AtomicUsize::new(0),
        }
    }

    pub fn single(signer: Arc<dyn BundlerSigner>) -> Self {
        Self::new(vec![signer])
    }

    pub fn count(&self) -> usize {
        self.signers.len()
    }

    pub fn addresses(&self) -> Vec<Address> {
        self.signers.iter().map(|s| s.address()).collect()
    }

    pub fn acquire(&self) -> SignerLease {
        let idx = self.next_index.fetch_add(1, Ordering::Relaxed) % self.signers.len();
        let signer = self.signers[idx].clone();
        SignerLease { signer, index: idx }
    }

    pub fn get(&self, index: usize) -> Option<Arc<dyn BundlerSigner>> {
        self.signers.get(index).cloned()
    }

    pub fn get_by_address(&self, address: &Address) -> Option<Arc<dyn BundlerSigner>> {
        self.signers
            .iter()
            .find(|s| s.address() == *address)
            .cloned()
    }
}

pub struct SignerLease {
    signer: Arc<dyn BundlerSigner>,
    index: usize,
}

impl SignerLease {
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub async fn sign_transaction(
        &self,
        tx: TransactionRequest,
        chain_id: u64,
    ) -> Result<Bytes, SignerError> {
        self.signer.sign_transaction(tx, chain_id).await
    }
}

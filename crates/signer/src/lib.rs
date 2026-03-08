mod local;
mod manager;

pub use local::LocalSigner;
pub use manager::{SignerLease, SignerManager};

use alloy_primitives::{Address, Bytes};
use alloy_rpc_types_eth::TransactionRequest;
use async_trait::async_trait;
use bunzo_types::error::SignerError;

#[async_trait]
pub trait BundlerSigner: Send + Sync + 'static {
    fn address(&self) -> Address;

    async fn sign_transaction(
        &self,
        tx: TransactionRequest,
        chain_id: u64,
    ) -> Result<Bytes, bunzo_types::error::SignerError>;
}

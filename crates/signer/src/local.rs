use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, Bytes, TxKind, U256};
use alloy_rpc_types_eth::TransactionRequest;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use async_trait::async_trait;
use bunzo_types::error::SignerError;

use crate::BundlerSigner;

pub struct LocalSigner {
    signer: PrivateKeySigner,
}

impl LocalSigner {
    pub fn from_private_key(key: &str) -> Result<Self, SignerError> {
        let signer: PrivateKeySigner =
            key.parse()
                .map_err(|e: alloy_signer_local::LocalSignerError| {
                    SignerError::SigningFailed(e.to_string())
                })?;
        Ok(Self { signer })
    }

    pub fn random() -> Self {
        Self {
            signer: PrivateKeySigner::random(),
        }
    }
}

#[async_trait]
impl BundlerSigner for LocalSigner {
    fn address(&self) -> Address {
        self.signer.address()
    }

    async fn sign_transaction(
        &self,
        tx: TransactionRequest,
        chain_id: u64,
    ) -> Result<Bytes, bunzo_types::error::SignerError> {
        let to = tx.to.unwrap_or(TxKind::Create);

        let eip1559_tx = TxEip1559 {
            chain_id,
            nonce: tx.nonce.unwrap_or(0),
            gas_limit: tx.gas.unwrap_or(0),
            max_fee_per_gas: tx.max_fee_per_gas.unwrap_or(0),
            max_priority_fee_per_gas: tx.max_priority_fee_per_gas.unwrap_or(0),
            to,
            value: tx.value.unwrap_or(U256::ZERO),
            access_list: Default::default(),
            input: tx.input.input().cloned().unwrap_or_default(),
        };

        let sig_hash = eip1559_tx.signature_hash();
        let sig = self
            .signer
            .sign_hash_sync(&sig_hash)
            .map_err(|e| SignerError::SigningFailed(e.to_string()))?;

        let signed = eip1559_tx.into_signed(sig);
        let envelope = alloy_consensus::TxEnvelope::Eip1559(signed);
        let mut buf = Vec::new();
        envelope.encode_2718(&mut buf);

        Ok(Bytes::from(buf))
    }
}

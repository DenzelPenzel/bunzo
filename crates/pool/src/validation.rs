use alloy_primitives::Address;
use tracing::debug;

use bunzo_types::chain::ChainSpec;
use bunzo_types::error::ValidationError;
use bunzo_types::user_operation::UserOperation as UserOperationTrait;
use bunzo_types::user_operation::v0_7::UserOperation;

use crate::pool::OperationPool;
use crate::reputation::{ReputationManager, ReputationStatus};
use std::sync::Arc;

pub struct Validator {
    chain_spec: ChainSpec,
    reputation: ReputationManager,
    pool: Option<Arc<OperationPool>>,
}

impl Validator {
    pub fn new(chain_spec: ChainSpec, reputation: ReputationManager) -> Self {
        Self {
            chain_spec,
            reputation,
            pool: None,
        }
    }

    pub fn with_pool(
        chain_spec: ChainSpec,
        reputation: ReputationManager,
        pool: Arc<OperationPool>,
    ) -> Self {
        Self {
            chain_spec,
            reputation,
            pool: Some(pool),
        }
    }

    pub fn validate_sync(&self, uo: &UserOperation, base_fee: u128) -> Result<(), ValidationError> {
        (|| {
            self.validate_static_fields(uo)?;
            self.validate_gas_price(uo, base_fee)?;
            self.validate_nonce(uo)?;
            self.validate_reputation(uo)?;
            Ok(())
        })()
    }

    fn validate_static_fields(&self, uo: &UserOperation) -> Result<(), ValidationError> {
        if uo.sender() == Address::ZERO {
            return Err(ValidationError::InvalidSender(Address::ZERO));
        }

        if uo.verification_gas_limit() == 0 {
            return Err(ValidationError::GasTooLow {
                field: "verificationGasLimit",
                value: 0,
                minimum: 1,
            });
        }

        if uo.max_fee_per_gas() == 0 {
            return Err(ValidationError::GasTooLow {
                field: "maxFeePerGas",
                value: 0,
                minimum: 1,
            });
        }

        if uo.max_priority_fee_per_gas() > uo.max_fee_per_gas() {
            return Err(ValidationError::PriorityFeeExceedsMaxFee {
                priority: uo.max_priority_fee_per_gas(),
                max_fee: uo.max_fee_per_gas(),
            });
        }

        // Call data size check
        let max_size = self.chain_spec.max_transaction_size_bytes;
        if uo.call_data().len() > max_size {
            return Err(ValidationError::CallDataTooLarge {
                size: uo.call_data().len(),
                max: max_size,
            });
        }

        if uo.pre_verification_gas() == 0 {
            return Err(ValidationError::GasTooLow {
                field: "preVerificationGas",
                value: 0,
                minimum: 1,
            });
        }

        debug!(sender = %uo.sender(), "static field validation passed");
        Ok(())
    }

    fn validate_gas_price(
        &self,
        uo: &UserOperation,
        base_fee: u128,
    ) -> Result<(), ValidationError> {
        let min_priority = self.chain_spec.min_max_priority_fee_per_gas;
        if !uo.gas_fees().covers(base_fee, min_priority) {
            return Err(ValidationError::MaxFeeTooLow(
                uo.max_fee_per_gas(),
                base_fee,
            ));
        }

        debug!(
            sender = %uo.sender(),
            max_fee = uo.max_fee_per_gas(),
            base_fee,
            "gas price validation passed"
        );
        Ok(())
    }

    /// Nonce validation against the current pool state.
    ///
    /// ERC-4337 v0.7 uses 2D nonces: `key (upper 192 bits) | sequence (lower 64 bits)`.
    /// For a given (sender, key), the EntryPoint requires that the sequence increments by 1.
    ///
    /// This layer checks:
    /// - If the pool has existing ops for this (sender, key), the new op's sequence must be
    ///   either the same (replacement) or the next consecutive value.
    /// - If no pool reference is available, the check is skipped.
    fn validate_nonce(&self, uo: &UserOperation) -> Result<(), ValidationError> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(()),
        };

        let id = uo.id();
        let nonce_key = id.nonce_key();
        let nonce_seq = id.nonce_sequence();

        // If an op with the same ID (sender + full nonce) exists, this is a replacement
        // The pool's add() method handles replacement fee validation
        if pool.get_by_id(&id).is_some() {
            debug!(
                sender = %id.sender,
                nonce_key = %nonce_key,
                sequence = nonce_seq,
                "nonce validation passed (replacement)"
            );
            return Ok(());
        }

        // Check the highest sequence in the pool for this sender+key
        if let Some(max_seq) = pool.max_nonce_sequence(&id.sender, &nonce_key) {
            // The new sequence must be exactly max_seq + 1 for consecutive ops
            if nonce_seq != max_seq + 1 {
                return Err(ValidationError::Other(format!(
                    "invalid nonce sequence: expected {} for sender {} key {}, got {}",
                    max_seq + 1,
                    id.sender,
                    nonce_key,
                    nonce_seq,
                )));
            }
        }

        // If no ops exist for this sender+key, any sequence is acceptable at the pool level
        debug!(
            sender = %id.sender,
            nonce_key = %nonce_key,
            sequence = nonce_seq,
            "nonce validation passed"
        );
        Ok(())
    }

    fn validate_reputation(&self, uo: &UserOperation) -> Result<(), ValidationError> {
        for entity in uo.entities() {
            if let Err(status) = self.reputation.check_entity(&entity) {
                let msg = match status {
                    ReputationStatus::Throttled => {
                        format!("{} is throttled", entity)
                    }
                    ReputationStatus::Banned => {
                        format!("{} is banned", entity)
                    }
                    _ => unreachable!(),
                };
                return Err(ValidationError::Other(msg));
            }
        }

        for entity in uo.entities() {
            self.reputation.record_seen(&entity.address);
        }

        Ok(())
    }

    pub fn reputation(&self) -> &ReputationManager {
        &self.reputation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Bytes, U256};

    fn test_chain_spec() -> ChainSpec {
        ChainSpec {
            min_max_priority_fee_per_gas: 0,
            ..ChainSpec::dev()
        }
    }

    fn make_valid_uo() -> UserOperation {
        UserOperation::new(
            Address::repeat_byte(0x01),
            U256::from(0),
            Bytes::from(vec![0xab]),
            100_000,
            200_000,
            50_000,
            30_000_000_000,
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
    fn test_valid_operation() {
        let validator = Validator::new(test_chain_spec(), ReputationManager::default());
        let uo = make_valid_uo();
        assert!(validator.validate_sync(&uo, 15_000_000_000).is_ok());
    }

    #[test]
    fn test_zero_sender_rejected() {
        let validator = Validator::new(test_chain_spec(), ReputationManager::default());
        let uo = UserOperation::new(
            Address::ZERO,
            U256::from(0),
            Bytes::new(),
            100_000,
            200_000,
            50_000,
            30_000_000_000,
            1_000_000_000,
            Bytes::new(),
            None,
            Bytes::new(),
            None,
            0,
            0,
            Bytes::new(),
            Address::ZERO,
            1,
        );
        assert!(matches!(
            validator.validate_sync(&uo, 15_000_000_000),
            Err(ValidationError::InvalidSender(_))
        ));
    }

    #[test]
    fn test_priority_fee_exceeds_max_fee() {
        let validator = Validator::new(test_chain_spec(), ReputationManager::default());
        let uo = UserOperation::new(
            Address::repeat_byte(0x01),
            U256::from(0),
            Bytes::from(vec![0xab]),
            100_000,
            200_000,
            50_000,
            1_000_000_000, // max fee
            2_000_000_000, // priority > max
            Bytes::from(vec![0x01]),
            None,
            Bytes::new(),
            None,
            0,
            0,
            Bytes::new(),
            Address::repeat_byte(0xEE),
            1,
        );
        assert!(matches!(
            validator.validate_sync(&uo, 500_000_000),
            Err(ValidationError::PriorityFeeExceedsMaxFee { .. })
        ));
    }

    #[test]
    fn test_max_fee_too_low() {
        let validator = Validator::new(test_chain_spec(), ReputationManager::default());
        let uo = UserOperation::new(
            Address::repeat_byte(0x01),
            U256::from(0),
            Bytes::from(vec![0xab]),
            100_000,
            200_000,
            50_000,
            1_000_000_000, // max fee = 1 gwei
            500_000_000,
            Bytes::from(vec![0x01]),
            None,
            Bytes::new(),
            None,
            0,
            0,
            Bytes::new(),
            Address::repeat_byte(0xEE),
            1,
        );
        // Base fee of 15 gwei > max fee of 1 gwei.
        assert!(matches!(
            validator.validate_sync(&uo, 15_000_000_000),
            Err(ValidationError::MaxFeeTooLow(_, _))
        ));
    }

    fn make_uo_with_nonce(nonce: U256) -> UserOperation {
        UserOperation::new(
            Address::repeat_byte(0x01),
            nonce,
            Bytes::from(vec![0xab]),
            100_000,
            200_000,
            50_000,
            30_000_000_000,
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
    fn test_nonce_first_op_accepted() {
        // First op for a sender+key is always accepted
        let pool = Arc::new(crate::pool::OperationPool::new(
            crate::pool::PoolConfig::default(),
        ));
        let validator = Validator::with_pool(test_chain_spec(), ReputationManager::default(), pool);
        let uo = make_uo_with_nonce(U256::from(0));
        assert!(validator.validate_sync(&uo, 15_000_000_000).is_ok());
    }

    #[test]
    fn test_nonce_consecutive_accepted() {
        // After adding nonce 0, nonce 1 should be accepted
        let pool = Arc::new(crate::pool::OperationPool::new(
            crate::pool::PoolConfig::default(),
        ));
        let uo0 = make_uo_with_nonce(U256::from(0));
        pool.add(uo0, 15_000_000_000).unwrap();

        let validator = Validator::with_pool(test_chain_spec(), ReputationManager::default(), pool);
        let uo1 = make_uo_with_nonce(U256::from(1));
        assert!(validator.validate_sync(&uo1, 15_000_000_000).is_ok());
    }

    #[test]
    fn test_nonce_gap_rejected() {
        // After adding nonce 0, nonce 2 (gap) should be rejected
        let pool = Arc::new(crate::pool::OperationPool::new(
            crate::pool::PoolConfig::default(),
        ));
        let uo0 = make_uo_with_nonce(U256::from(0));
        pool.add(uo0, 15_000_000_000).unwrap();

        let validator = Validator::with_pool(test_chain_spec(), ReputationManager::default(), pool);
        let uo2 = make_uo_with_nonce(U256::from(2));
        assert!(validator.validate_sync(&uo2, 15_000_000_000).is_err());
    }

    #[test]
    fn test_nonce_replacement_accepted() {
        // Adding an op with the same nonce (replacement) should pass nonce validation
        let pool = Arc::new(crate::pool::OperationPool::new(
            crate::pool::PoolConfig::default(),
        ));
        let uo0 = make_uo_with_nonce(U256::from(0));
        pool.add(uo0, 15_000_000_000).unwrap();

        let validator = Validator::with_pool(test_chain_spec(), ReputationManager::default(), pool);
        // Same nonce = replacement candidate. Nonce validation should pass
        // (pool.add() handles fee bump check separately)
        let uo0_replacement = make_uo_with_nonce(U256::from(0));
        assert!(
            validator
                .validate_sync(&uo0_replacement, 15_000_000_000)
                .is_ok()
        );
    }

    #[test]
    fn test_nonce_2d_different_keys() {
        // Ops with different nonce keys should be independent
        let pool = Arc::new(crate::pool::OperationPool::new(
            crate::pool::PoolConfig::default(),
        ));
        // Key 0, sequence 0
        let uo_k0_s0 = make_uo_with_nonce(U256::from(0));
        pool.add(uo_k0_s0, 15_000_000_000).unwrap();

        let validator = Validator::with_pool(test_chain_spec(), ReputationManager::default(), pool);
        // Key 1 (shifted left 64 bits), sequence 0 — should be accepted
        let key1_seq0 = U256::from(1) << 64;
        let uo_k1_s0 = make_uo_with_nonce(key1_seq0);
        assert!(validator.validate_sync(&uo_k1_s0, 15_000_000_000).is_ok());
    }
}

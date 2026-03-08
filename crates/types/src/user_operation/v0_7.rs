use alloy_primitives::{Address, Bytes, FixedBytes, B256, U256};
use alloy_sol_types::SolValue;
use serde::{Deserialize, Serialize};

use super::EntryPointVersion;

// ABI type for the packed user operation as it appears on-chain.
alloy_sol_macro::sol! {
    #[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct PackedUserOperation {
        address sender;
        uint256 nonce;
        bytes initCode;
        bytes callData;
        bytes32 accountGasLimits;
        uint256 preVerificationGas;
        bytes32 gasFees;
        bytes paymasterAndData;
        bytes signature;
    }
}

/// Off-chain representation of a v0.7 user operation with unpacked fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOperation {
    pub sender: Address,
    pub nonce: U256,
    pub call_data: Bytes,
    pub call_gas_limit: u128,
    pub verification_gas_limit: u128,
    pub pre_verification_gas: u128,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub signature: Bytes,

    // Optional fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory: Option<Address>,
    #[serde(default)]
    pub factory_data: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paymaster: Option<Address>,
    #[serde(default)]
    pub paymaster_verification_gas_limit: u128,
    #[serde(default)]
    pub paymaster_post_op_gas_limit: u128,
    #[serde(default)]
    pub paymaster_data: Bytes,

    // Cached fields (not serialized).
    #[serde(skip)]
    entry_point: Address,
    #[serde(skip)]
    chain_id: u64,
    #[serde(skip)]
    hash: Option<B256>,
}

impl UserOperation {
    /// Create a new user operation and compute its hash.
    pub fn new(
        sender: Address,
        nonce: U256,
        call_data: Bytes,
        call_gas_limit: u128,
        verification_gas_limit: u128,
        pre_verification_gas: u128,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
        signature: Bytes,
        factory: Option<Address>,
        factory_data: Bytes,
        paymaster: Option<Address>,
        paymaster_verification_gas_limit: u128,
        paymaster_post_op_gas_limit: u128,
        paymaster_data: Bytes,
        entry_point: Address,
        chain_id: u64,
    ) -> Self {
        let mut uo = Self {
            sender,
            nonce,
            call_data,
            call_gas_limit,
            verification_gas_limit,
            pre_verification_gas,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            signature,
            factory,
            factory_data,
            paymaster,
            paymaster_verification_gas_limit,
            paymaster_post_op_gas_limit,
            paymaster_data,
            entry_point,
            chain_id,
            hash: None,
        };
        uo.hash = Some(uo.compute_hash());
        uo
    }

    /// Pack this user operation into the on-chain format.
    pub fn pack(&self) -> PackedUserOperation {
        let account_gas_limits = pack_u128_pair(self.verification_gas_limit, self.call_gas_limit);
        let gas_fees = pack_u128_pair(self.max_priority_fee_per_gas, self.max_fee_per_gas);

        let init_code = match self.factory {
            Some(factory) => {
                let mut buf = factory.to_vec();
                buf.extend_from_slice(&self.factory_data);
                Bytes::from(buf)
            }
            None => Bytes::new(),
        };

        let paymaster_and_data = match self.paymaster {
            Some(paymaster) => {
                let mut buf = paymaster.to_vec();
                buf.extend_from_slice(&self.paymaster_verification_gas_limit.to_be_bytes());
                buf.extend_from_slice(&self.paymaster_post_op_gas_limit.to_be_bytes());
                buf.extend_from_slice(&self.paymaster_data);
                Bytes::from(buf)
            }
            None => Bytes::new(),
        };

        PackedUserOperation {
            sender: self.sender,
            nonce: self.nonce,
            initCode: init_code,
            callData: self.call_data.clone(),
            accountGasLimits: FixedBytes::from(account_gas_limits),
            preVerificationGas: U256::from(self.pre_verification_gas),
            gasFees: FixedBytes::from(gas_fees),
            paymasterAndData: paymaster_and_data,
            signature: self.signature.clone(),
        }
    }

    /// Unpack a `PackedUserOperation` into an off-chain `UserOperation`.
    pub fn unpack(
        packed: &PackedUserOperation,
        entry_point: Address,
        chain_id: u64,
    ) -> Self {
        let (verification_gas_limit, call_gas_limit) =
            unpack_u128_pair(&packed.accountGasLimits.0);
        let (max_priority_fee_per_gas, max_fee_per_gas) = unpack_u128_pair(&packed.gasFees.0);

        let (factory, factory_data) = if packed.initCode.len() >= 20 {
            let factory = Address::from_slice(&packed.initCode[..20]);
            let data = Bytes::copy_from_slice(&packed.initCode[20..]);
            (Some(factory), data)
        } else {
            (None, Bytes::new())
        };

        let (paymaster, pm_verification_gas, pm_post_op_gas, paymaster_data) =
            if packed.paymasterAndData.len() >= 52 {
                // 20 bytes address + 16 bytes verification gas + 16 bytes post-op gas.
                let paymaster = Address::from_slice(&packed.paymasterAndData[..20]);
                let mut vg_bytes = [0u8; 16];
                vg_bytes.copy_from_slice(&packed.paymasterAndData[20..36]);
                let mut pg_bytes = [0u8; 16];
                pg_bytes.copy_from_slice(&packed.paymasterAndData[36..52]);
                let pm_vg = u128::from_be_bytes(vg_bytes);
                let pm_pg = u128::from_be_bytes(pg_bytes);
                let data = Bytes::copy_from_slice(&packed.paymasterAndData[52..]);
                (Some(paymaster), pm_vg, pm_pg, data)
            } else {
                (None, 0, 0, Bytes::new())
            };

        Self::new(
            packed.sender,
            packed.nonce,
            packed.callData.clone(),
            call_gas_limit,
            verification_gas_limit,
            u128::try_from(packed.preVerificationGas).unwrap_or(u128::MAX),
            max_fee_per_gas,
            max_priority_fee_per_gas,
            packed.signature.clone(),
            factory,
            factory_data,
            paymaster,
            pm_verification_gas,
            pm_post_op_gas,
            paymaster_data,
            entry_point,
            chain_id,
        )
    }

    /// Compute the ERC-4337 operation hash.
    ///
    /// hash(pack(uo), entryPoint, chainId)
    fn compute_hash(&self) -> B256 {
        let packed = self.pack();

        // The "hash encoding" omits the signature: encode the struct with an empty signature,
        // then keccak the encoded bytes, then hash with entry point + chain id.
        let hash_packed = PackedUserOperation {
            signature: Bytes::new(),
            ..packed
        };

        let encoded = hash_packed.abi_encode();
        let inner_hash = alloy_primitives::keccak256(&encoded);

        // Final hash: keccak256(innerHash ++ entryPoint ++ chainId).
        let mut buf = Vec::with_capacity(84);
        buf.extend_from_slice(inner_hash.as_slice());
        buf.extend_from_slice(self.entry_point.as_slice());
        buf.extend_from_slice(&U256::from(self.chain_id).to_be_bytes::<32>());
        alloy_primitives::keccak256(&buf)
    }

    /// Set the entry point and chain ID, recomputing the hash.
    pub fn set_context(&mut self, entry_point: Address, chain_id: u64) {
        self.entry_point = entry_point;
        self.chain_id = chain_id;
        self.hash = Some(self.compute_hash());
    }
}

impl super::UserOperation for UserOperation {
    fn entry_point_version(&self) -> EntryPointVersion {
        EntryPointVersion::V0_7
    }

    fn sender(&self) -> Address {
        self.sender
    }

    fn nonce(&self) -> U256 {
        self.nonce
    }

    fn call_data(&self) -> &Bytes {
        &self.call_data
    }

    fn signature(&self) -> &Bytes {
        &self.signature
    }

    fn hash(&self) -> B256 {
        self.hash.unwrap_or_else(|| self.compute_hash())
    }

    fn factory(&self) -> Option<Address> {
        self.factory
    }

    fn paymaster(&self) -> Option<Address> {
        self.paymaster
    }

    fn call_gas_limit(&self) -> u128 {
        self.call_gas_limit
    }

    fn verification_gas_limit(&self) -> u128 {
        self.verification_gas_limit
    }

    fn pre_verification_gas(&self) -> u128 {
        self.pre_verification_gas
    }

    fn max_fee_per_gas(&self) -> u128 {
        self.max_fee_per_gas
    }

    fn max_priority_fee_per_gas(&self) -> u128 {
        self.max_priority_fee_per_gas
    }

    fn paymaster_verification_gas_limit(&self) -> u128 {
        self.paymaster_verification_gas_limit
    }

    fn paymaster_post_op_gas_limit(&self) -> u128 {
        self.paymaster_post_op_gas_limit
    }

    fn abi_encoded_size(&self) -> usize {
        self.pack().abi_encode().len()
    }

    fn calldata_gas_cost(&self, zero_byte_cost: u64, non_zero_byte_cost: u64) -> u128 {
        let encoded = self.pack().abi_encode();
        let cost: u64 = encoded.iter().fold(0u64, |acc, &byte| {
            if byte == 0 {
                acc + zero_byte_cost
            } else {
                acc + non_zero_byte_cost
            }
        });
        cost as u128
    }
}

/// A user operation with optional gas fields, used for `eth_estimateUserOperationGas`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOperationOptionalGas {
    pub sender: Address,
    pub nonce: U256,
    pub call_data: Bytes,
    pub signature: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory: Option<Address>,
    #[serde(default)]
    pub factory_data: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paymaster: Option<Address>,
    #[serde(default)]
    pub paymaster_data: Bytes,

    // All gas fields are optional for estimation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_gas_limit: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_gas_limit: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_verification_gas: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_priority_fee_per_gas: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paymaster_verification_gas_limit: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paymaster_post_op_gas_limit: Option<u128>,
}

impl UserOperationOptionalGas {
    /// Convert to a full `UserOperation` by filling in gas defaults.
    pub fn into_user_operation(
        self,
        entry_point: Address,
        chain_id: u64,
        max_verification_gas: u128,
        max_call_gas: u128,
    ) -> UserOperation {
        UserOperation::new(
            self.sender,
            self.nonce,
            self.call_data,
            self.call_gas_limit.unwrap_or(max_call_gas),
            self.verification_gas_limit.unwrap_or(max_verification_gas),
            self.pre_verification_gas.unwrap_or(0),
            self.max_fee_per_gas.unwrap_or(0),
            self.max_priority_fee_per_gas.unwrap_or(0),
            self.signature,
            self.factory,
            self.factory_data,
            self.paymaster,
            self.paymaster_verification_gas_limit.unwrap_or(0),
            self.paymaster_post_op_gas_limit.unwrap_or(0),
            self.paymaster_data,
            entry_point,
            chain_id,
        )
    }
}

/// Pack two u128 values into a 32-byte array (big-endian, high then low).
fn pack_u128_pair(high: u128, low: u128) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[..16].copy_from_slice(&high.to_be_bytes());
    buf[16..].copy_from_slice(&low.to_be_bytes());
    buf
}

/// Unpack a 32-byte array into two u128 values (big-endian, high then low).
fn unpack_u128_pair(bytes: &[u8; 32]) -> (u128, u128) {
    let mut high_bytes = [0u8; 16];
    let mut low_bytes = [0u8; 16];
    high_bytes.copy_from_slice(&bytes[..16]);
    low_bytes.copy_from_slice(&bytes[16..]);
    (u128::from_be_bytes(high_bytes), u128::from_be_bytes(low_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_operation::UserOperation as UserOperationTrait;

    #[test]
    fn test_pack_unpack_u128_pair() {
        let high = 1_000_000u128;
        let low = 2_000_000u128;
        let packed = pack_u128_pair(high, low);
        let (h, l) = unpack_u128_pair(&packed);
        assert_eq!(h, high);
        assert_eq!(l, low);
    }

    #[test]
    fn test_pack_roundtrip() {
        let uo = UserOperation::new(
            Address::repeat_byte(0x01),
            U256::from(42),
            Bytes::from(vec![0xde, 0xad]),
            100_000,
            200_000,
            50_000,
            30_000_000_000u128,
            1_000_000_000u128,
            Bytes::from(vec![0x01, 0x02]),
            Some(Address::repeat_byte(0x02)),
            Bytes::from(vec![0x03]),
            Some(Address::repeat_byte(0x03)),
            100_000,
            50_000,
            Bytes::from(vec![0x04]),
            Address::repeat_byte(0xEE),
            1,
        );

        let packed = uo.pack();
        let unpacked = UserOperation::unpack(&packed, Address::repeat_byte(0xEE), 1);

        assert_eq!(uo.sender, unpacked.sender);
        assert_eq!(uo.nonce, unpacked.nonce);
        assert_eq!(uo.call_gas_limit, unpacked.call_gas_limit);
        assert_eq!(uo.verification_gas_limit, unpacked.verification_gas_limit);
        assert_eq!(uo.max_fee_per_gas, unpacked.max_fee_per_gas);
        assert_eq!(uo.max_priority_fee_per_gas, unpacked.max_priority_fee_per_gas);
        assert_eq!(uo.factory, unpacked.factory);
        assert_eq!(uo.paymaster, unpacked.paymaster);
    }

    #[test]
    fn test_hash_deterministic() {
        let uo = UserOperation::new(
            Address::repeat_byte(0x01),
            U256::from(1),
            Bytes::from(vec![0xab]),
            100_000,
            200_000,
            50_000,
            30_000_000_000u128,
            1_000_000_000u128,
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

        let h1 = uo.hash();
        let h2 = uo.hash();
        assert_eq!(h1, h2);
        assert_ne!(h1, B256::ZERO);
    }

    #[test]
    fn test_id() {
        let uo = UserOperation::new(
            Address::repeat_byte(0x01),
            U256::from(7),
            Bytes::new(),
            0, 0, 0, 0, 0,
            Bytes::new(),
            None, Bytes::new(),
            None, 0, 0, Bytes::new(),
            Address::ZERO, 1,
        );
        let id = uo.id();
        assert_eq!(id.sender, Address::repeat_byte(0x01));
        assert_eq!(id.nonce, U256::from(7));
    }
}

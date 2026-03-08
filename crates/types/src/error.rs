use alloy_primitives::{Address, B256};

#[derive(Debug, thiserror::Error)]
pub enum BunzoError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("pool error: {0}")]
    Pool(#[from] PoolError),

    #[error("builder error: {0}")]
    Builder(#[from] BuilderError),

    #[error("signer error: {0}")]
    Signer(#[from] SignerError),

    #[error("rpc error: {0}")]
    Rpc(#[from] RpcError),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("rpc transport error: {0}")]
    Transport(String),

    #[error("contract revert: {0}")]
    ContractRevert(String),

    #[error("entry point error: {0}")]
    EntryPoint(String),

    #[error("simulation failed: {0}")]
    SimulationFailed(String),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("operation already known: {0}")]
    AlreadyKnown(B256),

    #[error("replacement underpriced: existing fee {existing}, new fee {new}")]
    ReplacementUnderpriced { existing: u128, new: u128 },

    #[error("pool is full (max {max_size})")]
    PoolFull { max_size: usize },

    #[error("validation failed: {0}")]
    Validation(#[from] ValidationError),

    #[error("entity {entity} is throttled")]
    EntityThrottled { entity: String },

    #[error("operation not found: {0}")]
    NotFound(B256),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("invalid sender: {0}")]
    InvalidSender(Address),

    #[error("invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u128, got: u128 },

    #[error("gas limit too low: {field} = {value}, minimum = {minimum}")]
    GasTooLow {
        field: &'static str,
        value: u128,
        minimum: u128,
    },

    #[error("gas limit too high: {field} = {value}, maximum = {maximum}")]
    GasTooHigh {
        field: &'static str,
        value: u128,
        maximum: u128,
    },

    #[error("max fee per gas too low: {0}, base fee: {1}")]
    MaxFeeTooLow(u128, u128),

    #[error("max priority fee exceeds max fee: priority={priority}, max={max_fee}")]
    PriorityFeeExceedsMaxFee { priority: u128, max_fee: u128 },

    #[error("paymaster deposit too low: {0}")]
    PaymasterDepositTooLow(Address),

    #[error("call data too large: {size} bytes, max {max}")]
    CallDataTooLarge { size: usize, max: usize },

    #[error("simulation reverted: {0}")]
    SimulationReverted(String),

    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    #[error("unstaked entity used forbidden opcode: {0}")]
    ForbiddenOpcode(String),

    #[error("unstaked entity accessed forbidden storage: {0}")]
    ForbiddenStorageAccess(String),

    #[error("expired or not yet valid: valid_after={valid_after}, valid_until={valid_until}")]
    ExpiredOrNotDue { valid_after: u64, valid_until: u64 },

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    #[error("no operations to bundle")]
    NothingToBundle,

    #[error("nonce conflict: {0}")]
    NonceConflict(String),

    #[error("submission failed: {0}")]
    SubmissionFailed(String),

    #[error("fee escalation exhausted after {attempts} attempts")]
    FeeEscalationExhausted { attempts: u32 },

    #[error("transaction reverted: {0}")]
    TransactionReverted(String),

    #[error("bundle reorged out at block {block}")]
    ReorgedOut { block: u64 },

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SignerError {
    #[error("no signer available")]
    NoSigner,

    #[error("signing failed: {0}")]
    SigningFailed(String),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("invalid params: {0}")]
    InvalidParams(String),

    #[error("method not found: {0}")]
    MethodNotFound(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("{0}")]
    Other(String),
}
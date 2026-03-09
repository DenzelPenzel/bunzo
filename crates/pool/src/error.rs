use alloy_primitives::B256;

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("operation already known: {0}")]
    AlreadyKnown(B256),

    #[error("replacement underpriced: existing fee {existing}, new fee {new}")]
    ReplacementUnderpriced { existing: u128, new: u128 },

    #[error("pool is full (max {max_size})")]
    PoolFull { max_size: usize },

    #[error("validation failed: {0}")]
    Validation(#[from] bunzo_types::error::ValidationError),

    #[error("entity throttled: {0}")]
    EntityThrottled(String),

    #[error("operation not found: {0}")]
    NotFound(B256),

    #[error("{0}")]
    Other(String),
}

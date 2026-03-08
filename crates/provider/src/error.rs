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

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type ProviderResult<T> = Result<T, ProviderError>;

use jsonrpsee::types::ErrorObjectOwned;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("invalid params: {0}")]
    InvalidParams(String),

    #[error("method not found: {0}")]
    MethodNotFound(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("pool error: {0}")]
    Pool(#[from] bunzo_pool::PoolError),

    #[error("{0}")]
    Other(String),
}

pub const EXECUTION_REVERTED: i32 = -32521;
pub const INVALID_USEROPERATION: i32 = -32602;

impl From<RpcError> for ErrorObjectOwned {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::InvalidParams(msg) => {
                ErrorObjectOwned::owned(INVALID_USEROPERATION, msg, None::<()>)
            }
            RpcError::MethodNotFound(msg) => ErrorObjectOwned::owned(-32601, msg, None::<()>),
            RpcError::Internal(msg) => ErrorObjectOwned::owned(-32603, msg, None::<()>),
            RpcError::Pool(err) => {
                ErrorObjectOwned::owned(INVALID_USEROPERATION, err.to_string(), None::<()>)
            }
            RpcError::Other(msg) => ErrorObjectOwned::owned(-32603, msg, None::<()>),
        }
    }
}

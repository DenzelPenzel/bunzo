use std::net::SocketAddr;
use std::sync::Arc;

use jsonrpsee::server::{Server, ServerHandle};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use bunzo_pool::{OperationPool, Validator};
use bunzo_provider::traits::{EntryPointProvider, EvmProvider};
use bunzo_types::chain::ChainSpec;

use crate::eth::{EthApiImpl, EthApiServer};

#[derive(Clone, Debug)]
pub struct RpcServerConfig {
    pub addr: SocketAddr,
    pub max_connections: u32,
    pub max_request_body_size: u32,
}

impl Default for RpcServerConfig {
    fn default() -> Self {
        Self {
            addr: ([127, 0, 0, 1], 3000).into(),
            max_connections: 100,
            max_request_body_size: 10 * 1024 * 1024, // 10 MB
        }
    }
}

pub struct RpcServer<P: EvmProvider = (), E: EntryPointProvider = ()> {
    config: RpcServerConfig,
    eth_api: EthApiImpl<P, E>,
}

impl RpcServer<(), ()> {
    pub fn new(
        config: RpcServerConfig,
        chain_spec: ChainSpec,
        pool: Arc<OperationPool>,
        validator: Arc<Validator>,
    ) -> Self {
        Self {
            config,
            eth_api: EthApiImpl::new(chain_spec, pool, validator),
        }
    }
}

impl<P: EvmProvider, E: EntryPointProvider> RpcServer<P, E> {
    pub fn with_providers(
        config: RpcServerConfig,
        chain_spec: ChainSpec,
        pool: Arc<OperationPool>,
        validator: Arc<Validator>,
        provider: Arc<P>,
        entry_point_provider: Arc<E>,
    ) -> Self {
        Self {
            config,
            eth_api: EthApiImpl::with_providers(
                chain_spec,
                pool,
                validator,
                provider,
                entry_point_provider,
            ),
        }
    }

    pub async fn start(self) -> anyhow::Result<(SocketAddr, ServerHandle)> {
        let cors = CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_origin(Any);

        let middleware = ServiceBuilder::new().layer(cors);

        let server = Server::builder()
            .max_connections(self.config.max_connections)
            .max_request_body_size(self.config.max_request_body_size)
            .set_http_middleware(middleware)
            .build(self.config.addr)
            .await?;

        let addr = server.local_addr()?;
        let handle = server.start(self.eth_api.into_rpc());

        info!(addr = %addr, "JSON-RPC server started");

        Ok((addr, handle))
    }
}

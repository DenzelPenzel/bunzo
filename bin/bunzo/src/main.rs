use std::net::SocketAddr;
use std::sync::Arc;

use alloy_primitives::Address;
use alloy_provider::ProviderBuilder;
use clap::Parser;
use tracing::{info, warn};

use bunzo_builder::{BundleProposerImpl, BundlerTask};
use bunzo_builder::strategy::DirectSubmissionStrategy;
use bunzo_pool::{OperationPool, PoolConfig, ReputationManager, Validator};
use bunzo_provider::{AlloyBundleHandler, AlloyEntryPointProvider, AlloyEvmProvider, EvmProvider};
use bunzo_provider::gas_oracle::ProviderGasOracle;
use bunzo_rpc::{RpcServer, RpcServerConfig};
use bunzo_signer::LocalSigner;
use bunzo_types::chain::ChainSpec;

#[derive(Parser, Debug)]
#[command(name = "bunzo", version, about = "ERC-4337 bundler")]
struct CLIArgs {
    /// JSON-RPC server listen address
    #[arg(long, env = "BUNZO_RPC_ADDR", default_value = "127.0.0.1:3000")]
    rpc_addr: SocketAddr,

    /// Ethereum node RPC URL
    #[arg(long, env = "BUNZO_NODE_URL", default_value = "http://localhost:8545")]
    node_url: String,

    /// Chain ID (0 = auto-detect from node)
    #[arg(long, env = "BUNZO_CHAIN_ID", default_value = "0")]
    chain_id: u64,

    /// EntryPoint v0.7 contract address
    #[arg(
        long,
        env = "BUNZO_ENTRY_POINT",
        default_value = "0x0000000071727De22E5E9d8BAf0edAc6f37da032"
    )]
    entry_point: Address,

    /// Bundler signer private key (hex, with or without 0x prefix)
    #[arg(long, env = "BUNZO_PRIVATE_KEY")]
    private_key: Option<String>,

    /// Maximum pool size
    #[arg(long, env = "BUNZO_MAX_POOL_SIZE", default_value = "4096")]
    max_pool_size: usize,

    /// Log level filter (e.g., "info", "debug", "bunzo=debug,info")
    #[arg(long, env = "BUNZO_LOG", default_value = "info")]
    log: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = CLIArgs::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log)),
        )
        .init();

    let url: alloy_transport_http::reqwest::Url = cli.node_url.parse()?;
    let alloy_provider = ProviderBuilder::new().connect_http(url);
    let evm_provider = Arc::new(AlloyEvmProvider::new(alloy_provider.clone()));

    let chain_id = if cli.chain_id == 0 {
        let detected = evm_provider.get_chain_id().await?;
        detected
    } else {
        cli.chain_id
    };

    let mut chain_spec = match chain_id {
        1 => ChainSpec::mainnet(),
        31337 => ChainSpec::dev(),
        _ => {
            let mut spec = ChainSpec::mainnet();
            spec.id = chain_id;
            spec.name = format!("chain-{}", chain_id);
            spec
        }
    };
    chain_spec.entry_point_v0_7 = cli.entry_point;
    chain_spec.id = chain_id;
    chain_spec.max_pool_size = cli.max_pool_size;

    let pool_config = PoolConfig::from(&chain_spec);
    let pool = Arc::new(OperationPool::new(pool_config));
    let reputation = ReputationManager::default();
    let validator = Arc::new(Validator::with_pool(chain_spec.clone(), reputation, pool.clone()));

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());

    if let Some(ref prv_key) = cli.private_key {
        let signer = Arc::new(LocalSigner::from_private_key(prv_key)?);
        info!(
            address = %bunzo_signer::BundlerSigner::address(signer.as_ref()),
            "bundler signer loaded"
        );

        let bundle_provider = ProviderBuilder::new().connect_http(cli.node_url.parse()?);
        let bundle_handler = AlloyBundleHandler::new(bundle_provider, cli.entry_point);

        let gas_oracle_provider = ProviderBuilder::new().connect_http(cli.node_url.parse()?);
        let gas_oracle_evm = AlloyEvmProvider::new(gas_oracle_provider);
        let gas_oracle = ProviderGasOracle::new(gas_oracle_evm);

        let beneficiary = bunzo_signer::BundlerSigner::address(signer.as_ref());
        let proposer = BundleProposerImpl::new(
            pool.clone(),
            bundle_handler,
            gas_oracle,
            chain_spec.clone(),
            beneficiary,
        );

        let strategy = DirectSubmissionStrategy::new(evm_provider.clone());

        let bundler_task = BundlerTask::new(
            proposer,
            signer,
            strategy,
            pool.clone(),
            evm_provider.clone(),
            chain_spec.clone(),
        );

        tokio::spawn(bundler_task.run(shutdown_rx));
        info!("bundler task started");
    } else {
        warn!("no private key provided — running in RPC-only mode (no bundling)");
    }

    let rpc_ep_provider = ProviderBuilder::new().connect_http(cli.node_url.parse()?);
    let entry_point_provider = Arc::new(AlloyEntryPointProvider::new(rpc_ep_provider, cli.entry_point));

    let rpc_config = RpcServerConfig {
        addr: cli.rpc_addr,
        ..RpcServerConfig::default()
    };

    let rpc_server = RpcServer::with_providers(
        rpc_config,
        chain_spec,
        pool.clone(),
        validator,
        evm_provider.clone(),
        entry_point_provider,
    );

    let (addr, handle) = rpc_server.start().await?;

    info!(addr = %addr, "bunzo is ready");


    tokio::signal::ctrl_c().await?;
    info!("shutting down");

    let _ = shutdown_tx.send(());
    handle.stop()?;

    Ok(())
}

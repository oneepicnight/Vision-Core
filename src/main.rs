//! vision-core - minimal stable blockchain node
//!
//! main.rs is responsible only for startup wiring:
//! - parse settings from environment
//! - print startup banner
//! - initialise chain state and genesis
//! - start API and P2P services
//! - block on the Tokio runtime
//!
//! All protocol logic lives in the dedicated modules under `src/`.

mod api;
mod chain;
mod config;
mod genesis;
mod mempool;
mod miner;
mod node;
mod p2p;
mod pow;
mod tests;
mod types;

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use config::constants::*;
use config::settings::Settings;

fn main() {
    let rt = match node::runtime::build_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Fatal: {error}");
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(async_main()) {
        tracing::error!("Fatal: {}", e);
        std::process::exit(1);
    }
}

async fn async_main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let settings = Settings::from_env();

    print_banner(&settings);

    let chain_state = node::bootstrap::initialize_chain_state(&settings)?;

    let chain = Arc::new(Mutex::new(chain_state));
    let peer_manager = Arc::new(p2p::peer_manager::PeerManager::new());
    let mempool = Arc::new(mempool::Mempool::new());
    let miner_manager = settings
        .mining_enabled
        .then(|| Arc::new(miner::MinerManager::new(*pow::visionx::VISIONX_PARAMS)));
    let recovery_state = Arc::new(node::recovery::RecoveryState::new());

    let seed_addrs = node::bootstrap::seed_peers(&settings);
    for addr in &seed_addrs {
        peer_manager.upsert(addr, true);
    }
    tracing::info!("[NODE] {} seed peers loaded", seed_addrs.len());

    node::services::start_services(
        chain.clone(),
        peer_manager.clone(),
        mempool.clone(),
        miner_manager.clone(),
        recovery_state.clone(),
        &settings,
    )
    .await?;

    let mut api_state = api::state::NodeApiState::new(chain.clone(), mempool)
        .with_peer_manager(peer_manager.clone())
        .with_recovery_state(recovery_state.clone())
        .with_alpha_airdrop_enabled(settings.alpha_airdrop_enabled);
    if let Some(miner_manager) = miner_manager {
        api_state = api_state.with_miner_manager(miner_manager);
    }
    let app = api::routes::api_router(api_state);
    let http_addr: std::net::SocketAddr = settings.http_addr.parse()?;
    tracing::info!("[API] Listening on http://{}", http_addr);

    axum::serve(tokio::net::TcpListener::bind(http_addr).await?, app).await?;

    Ok(())
}

fn print_banner(settings: &Settings) {
    println!(
        r#"
+--------------------------------------------------------------+
|                      vision-core {}                       |
|
|  Network  : {}
|  P2P      : {}
|  API      : {}
|  Mining   : {}
|  Data     : {}
+--------------------------------------------------------------+
"#,
        NODE_VERSION,
        NETWORK_ID,
        settings.p2p_addr,
        settings.http_addr,
        if settings.mining_enabled {
            "enabled"
        } else {
            "disabled"
        },
        settings.data_dir,
    );
    tracing::info!("vision-core {} starting", NODE_VERSION);
}

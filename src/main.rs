//! vision-core â€” minimal stable blockchain node
//!
//! `main.rs` is responsible only for startup wiring:
//! - Parse settings from environment
//! - Print startup banner
//! - Initialise chain state and genesis
//! - Start API and P2P services
//! - Block on the Tokio runtime
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
use tokio::sync::Mutex;
use anyhow::Result;
use tracing_subscriber::EnvFilter;

use config::constants::*;
use config::settings::Settings;

// â”€â”€â”€ Entry point â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn main() {
    let rt = node::runtime::build_runtime();
    if let Err(e) = rt.block_on(async_main()) {
        tracing::error!("Fatal: {}", e);
        std::process::exit(1);
    }
}

async fn async_main() -> Result<()> {
    // â”€â”€ Logging â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // â”€â”€ Settings â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let settings = Settings::from_env();

    // â”€â”€ Banner â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    print_banner(&settings);

    // â”€â”€ Chain state â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let mut chain_state = chain::state::ChainState::open(&settings.data_dir)?;

    // â”€â”€ Bootstrap (genesis validation + DB init) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    node::bootstrap::bootstrap_chain(&mut chain_state, &settings)?;

    let chain = Arc::new(Mutex::new(chain_state));
    let peer_manager = Arc::new(p2p::peer_manager::PeerManager::new());
    let mempool = Arc::new(mempool::Mempool::new());
    let miner_manager = settings.mining_enabled.then(|| {
        Arc::new(miner::MinerManager::new(*pow::visionx::VISIONX_PARAMS))
    });

    // â”€â”€ Seed peers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let seed_addrs = node::bootstrap::seed_peers(&settings);
    for addr in &seed_addrs {
        peer_manager.upsert(addr, true);
    }
    tracing::info!("[NODE] {} seed peers loaded", seed_addrs.len());

    // â”€â”€ Background services â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    node::services::start_services(chain.clone(), peer_manager.clone(), &settings).await?;

    // â”€â”€ HTTP API â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
    let mut api_state = api::state::NodeApiState::new(chain.clone(), mempool)
        .with_peer_manager(peer_manager.clone())
        .with_alpha_airdrop_enabled(settings.alpha_airdrop_enabled);
    if let Some(miner_manager) = miner_manager {
        api_state = api_state.with_miner_manager(miner_manager);
    }
    let app = api::routes::api_router(api_state);
    let http_addr: std::net::SocketAddr = settings.http_addr.parse()?;
    tracing::info!("[API] Listening on http://{}", http_addr);

    axum::serve(
        tokio::net::TcpListener::bind(http_addr).await?,
        app,
    )
    .await?;

    Ok(())
}

// â”€â”€â”€ Startup banner â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

fn print_banner(settings: &Settings) {
    println!(
        r#"
â•”â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•—
â•‘            vision-core  {}
â•‘
â•‘  Network  : {}
â•‘  P2P      : {}
â•‘  API      : {}
â•‘  Mining   : {}
â•‘  Data     : {}
â•šâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
"#,
        NODE_VERSION,
        NETWORK_ID,
        settings.p2p_addr,
        settings.http_addr,
        if settings.mining_enabled { "enabled" } else { "disabled" },
        settings.data_dir,
    );
    tracing::info!("vision-core {} starting", NODE_VERSION);
}






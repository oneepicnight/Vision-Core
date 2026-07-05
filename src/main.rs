//! vision-core — minimal stable blockchain node
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

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let rt = node::runtime::build_runtime();
    if let Err(e) = rt.block_on(async_main()) {
        tracing::error!("Fatal: {}", e);
        std::process::exit(1);
    }
}

async fn async_main() -> Result<()> {
    // ── Logging ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // ── Settings ──────────────────────────────────────────────────────────────
    let settings = Settings::from_env();

    // ── Banner ────────────────────────────────────────────────────────────────
    print_banner(&settings);

    // ── Chain state ───────────────────────────────────────────────────────────
    let mut chain_state = chain::state::ChainState::open(&settings.data_dir)?;

    // ── Bootstrap (genesis validation + DB init) ──────────────────────────────
    node::bootstrap::bootstrap_chain(&mut chain_state, &settings)?;

    let chain = Arc::new(Mutex::new(chain_state));
    let peer_manager = Arc::new(p2p::peer_manager::PeerManager::new());

    // ── Seed peers ────────────────────────────────────────────────────────────
    let seed_addrs = node::bootstrap::seed_peers(&settings);
    for addr in &seed_addrs {
        peer_manager.upsert(addr, true);
    }
    tracing::info!("[NODE] {} seed peers loaded", seed_addrs.len());

    // ── Background services ───────────────────────────────────────────────────
    node::services::start_services(chain.clone(), peer_manager.clone(), &settings).await?;

    // ── HTTP API ──────────────────────────────────────────────────────────────
    let app = api::routes::api_router();
    let http_addr: std::net::SocketAddr = settings.http_addr.parse()?;
    tracing::info!("[API] Listening on http://{}", http_addr);

    axum::serve(
        tokio::net::TcpListener::bind(http_addr).await?,
        app,
    )
    .await?;

    Ok(())
}

// ─── Startup banner ───────────────────────────────────────────────────────────

fn print_banner(settings: &Settings) {
    println!(
        r#"
╔══════════════════════════════════════════════════════╗
║            vision-core  {}
║
║  Network  : {}
║  P2P      : {}
║  API      : {}
║  Mining   : {}
║  Data     : {}
╚══════════════════════════════════════════════════════╝
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

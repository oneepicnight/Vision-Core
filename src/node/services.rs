use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;

use crate::chain::ChainState;
use crate::config::settings::Settings;
use crate::p2p::connection::P2PConnectionManager;
use crate::p2p::peer_manager::PeerManager;

/// Spawn all background services for a running node.
///
/// Each service runs in its own Tokio task. Services communicate via shared
/// Arc<Mutex<>> handles or Tokio channels.
pub async fn start_services(
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    settings: &Settings,
) -> Result<()> {
    // ── P2P listener ──────────────────────────────────────────────────────────
    let p2p_addr: SocketAddr = settings.p2p_addr.parse()?;
    let conn_mgr = Arc::new(P2PConnectionManager::new(p2p_addr, chain.clone(), peer_manager.clone()));
    {
        let mgr = conn_mgr.clone();
        tokio::spawn(async move {
            if let Err(e) = mgr.run_listener().await {
                tracing::error!("[P2P] Listener error: {}", e);
            }
        });
    }

    // ── Sync watchdog ─────────────────────────────────────────────────────────
    {
        let pm = peer_manager.clone();
        let chain_ref = chain.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(20)
            );
            let mut sync_guard = crate::p2p::sync::SyncGuard::new();
            loop {
                interval.tick().await;
                let local_height = chain_ref.lock().await.current_height();
                if let Err(e) = crate::p2p::sync::watchdog_step(&pm, local_height, &mut sync_guard).await {
                    tracing::warn!("[SYNC] Watchdog error: {}", e);
                }
            }
        });
    }

    // ── Mining service ────────────────────────────────────────────────────────
    if settings.mining_enabled {
        tracing::info!("[MINER] Mining enabled");
        // Mining task spawn will be wired here once MinerManager is integrated.
    }

    tracing::info!("[NODE] All services started");
    Ok(())
}


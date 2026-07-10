use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::sync::Mutex;

use crate::chain::accept::{apply_coinbase_reward, AcceptResult};
use crate::chain::state_root::compute_state_root;
use crate::chain::ChainState;
use crate::config::constants::{BLOCK_TARGET_TXS, MIN_PEERS_FOR_MINING};
use crate::config::settings::Settings;
use crate::mempool::Mempool;
use crate::miner::MinerManager;
use crate::p2p::connection::{recv_message, send_message, P2PConnectionManager};
use crate::p2p::messages::P2PMessage;
use crate::p2p::peer_manager::{PeerManager, PeerState};
use crate::p2p::protocol::{validate_handshake, HandshakeMessage, HandshakeResult};
use crate::types::transaction::{canonical_tx_id, simulate_tx_execution, TxExecutionState};

/// Spawn all background services for a running node.
///
/// Each service runs in its own Tokio task. Services communicate via shared
/// Arc<Mutex<>> handles or Tokio channels.
pub async fn start_services(
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    mempool: Arc<Mempool>,
    miner_manager: Option<Arc<MinerManager>>,
    settings: &Settings,
) -> Result<()> {
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

    if !settings.seed_peers.is_empty() {
        for seed_peer in settings.seed_peers.clone() {
            let chain_ref = chain.clone();
            let peer_manager_ref = peer_manager.clone();
            let conn_mgr_ref = conn_mgr.clone();
            tokio::spawn(async move {
                seed_peer_loop(chain_ref, peer_manager_ref, conn_mgr_ref, seed_peer).await;
            });
        }
    }

    {
        let mgr = conn_mgr.clone();
        let pm = peer_manager.clone();
        let chain_ref = chain.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(20));
            let mut sync_guard = crate::p2p::sync::SyncGuard::new();
            loop {
                interval.tick().await;
                if let Err(e) = crate::p2p::sync::watchdog_step(&mgr, &chain_ref, &pm, &mut sync_guard).await {
                    tracing::warn!("[SYNC] Watchdog error: {}", e);
                }
            }
        });
    }

    if settings.mining_enabled {
        tracing::info!("[MINER] Mining enabled");
        if let Some(miner_manager) = miner_manager {
            let chain_ref = chain.clone();
            let peer_manager_ref = peer_manager.clone();
            let mempool_ref = mempool.clone();
            let miner_addr = settings.miner_address.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(250));
                loop {
                    interval.tick().await;

                    if peer_manager_ref.connected_count() < MIN_PEERS_FOR_MINING {
                        miner_manager.clear_job();
                        continue;
                    }

                    let maybe_job = {
                        let chain_guard = chain_ref.lock().await;
                        let txs = mempool_ref.select_for_block(BLOCK_TARGET_TXS);
                        (|| -> Option<crate::miner::job::MiningJob> {
                            let mut job = miner_manager
                                .build_candidate_for_tip(&chain_guard, &miner_addr, txs)?;
                            let mut exec_state = TxExecutionState::from_balances_and_nonces(
                                chain_guard.balances.clone(),
                                chain_guard.nonces.clone(),
                            );
                            for tx in job.txs.iter().skip(1) {
                                simulate_tx_execution(&mut exec_state, tx).ok()?;
                            }
                            apply_coinbase_reward(
                                &mut exec_state,
                                &job.header_template.miner,
                                job.header_template.number,
                            )
                            .ok()?;
                            let state_root = compute_state_root(&exec_state.balances, &exec_state.nonces).ok()?;
                            job.header_template.state_root = state_root;
                            Some(job)
                        })()
                    };

                    let Some(job) = maybe_job else {
                        continue;
                    };

                    miner_manager.build_job(job.clone());

                    let mined = tokio::task::spawn_blocking(move || {
                        let mut nonce = 0u64;
                        loop {
                            if let Some(block) = job.try_nonce(nonce) {
                                return Some(block);
                            }
                            if nonce == u64::MAX {
                                return None;
                            }
                            nonce = nonce.wrapping_add(1);
                        }
                    })
                    .await
                    .unwrap_or(None);

                    let Some(block) = mined else {
                        continue;
                    };

                    let confirmed_tx_ids: Vec<String> = block.txs.iter().map(canonical_tx_id).collect();
                    let mut chain_guard = chain_ref.lock().await;
                    match miner_manager.submit_solution(&mut chain_guard, block) {
                        AcceptResult::CanonExtension { .. } => {
                            mempool_ref.remove_confirmed(&confirmed_tx_ids);
                            tracing::info!(
                                "[MINER] accepted mined block and cleared {} txs",
                                confirmed_tx_ids.len()
                            );
                        }
                        AcceptResult::SideChain { .. } => {
                            tracing::debug!("[MINER] mined block landed on a side chain");
                        }
                        AcceptResult::StoredOrphan { block_hash } => {
                            tracing::debug!("[MINER] mined block stored as orphan {}", block_hash);
                        }
                        AcceptResult::Rejected(reason) => {
                            tracing::debug!("[MINER] mined block rejected: {}", reason);
                        }
                    }
                }
            });
        } else {
            tracing::warn!("[MINER] mining enabled but miner manager is unavailable");
        }
    }

    tracing::info!("[NODE] All services started");
    Ok(())
}

async fn seed_peer_loop(
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    conn_mgr: Arc<P2PConnectionManager>,
    peer_addr: String,
) {
    let reconnect_delay = Duration::from_secs(2);
    let heartbeat_delay = Duration::from_secs(5);

    let peer_socket: SocketAddr = match peer_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
            tracing::warn!("[P2P] invalid seed peer {}: {}", peer_addr, e);
            return;
        }
    };

    loop {
        match P2PConnectionManager::connect(peer_socket).await {
            Ok(mut stream) => {
                let local_nonce = conn_mgr.local_node_nonce();
                let local_height = chain.lock().await.current_height();
                let local_hs = HandshakeMessage::new(local_height, local_nonce);

                if let Err(e) = send_message(&mut stream, &P2PMessage::Handshake(local_hs)).await {
                    tracing::warn!("[P2P] {} handshake send error: {}", peer_addr, e);
                    peer_manager.set_state(&peer_addr, PeerState::Disconnected);
                    tokio::time::sleep(reconnect_delay).await;
                    continue;
                }

                let remote_hs = match recv_message(&mut stream).await {
                    Ok(P2PMessage::Handshake(hs)) => hs,
                    Ok(P2PMessage::Disconnect { reason }) => {
                        tracing::warn!("[P2P] {} rejected connection: {}", peer_addr, reason);
                        peer_manager.set_state(&peer_addr, PeerState::Disconnected);
                        tokio::time::sleep(reconnect_delay).await;
                        continue;
                    }
                    Ok(other) => {
                        tracing::warn!("[P2P] {} unexpected reply during handshake: {}", peer_addr, other.label());
                        peer_manager.set_state(&peer_addr, PeerState::Disconnected);
                        tokio::time::sleep(reconnect_delay).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("[P2P] {} handshake read error: {}", peer_addr, e);
                        peer_manager.set_state(&peer_addr, PeerState::Disconnected);
                        tokio::time::sleep(reconnect_delay).await;
                        continue;
                    }
                };

                match validate_handshake(&remote_hs, local_nonce) {
                    HandshakeResult::Accepted => {
                        peer_manager.upsert(&peer_addr, true);
                        peer_manager.set_state(&peer_addr, PeerState::Connected);
                        peer_manager.note_peer_height(&peer_addr, remote_hs.chain_height, false);
                    }
                    other => {
                        let reason = match other {
                            HandshakeResult::VersionMismatch { remote, ours } => {
                                format!("unsupported protocol version: remote={} ours={}", remote, ours)
                            }
                            HandshakeResult::WrongChainId => "wrong chain identity".to_string(),
                            HandshakeResult::WrongGenesisHash => "wrong genesis hash".to_string(),
                            HandshakeResult::WrongEconHash => "wrong economic version".to_string(),
                            HandshakeResult::WrongPowParams => "wrong pow/consensus version".to_string(),
                            HandshakeResult::SelfConnection => "self-connection rejected".to_string(),
                            HandshakeResult::Accepted => "handshake accepted".to_string(),
                        };
                        let _ = send_message(&mut stream, &P2PMessage::Disconnect { reason }).await;
                        peer_manager.set_state(&peer_addr, PeerState::Disconnected);
                        tokio::time::sleep(reconnect_delay).await;
                        continue;
                    }
                }

                loop {
                    tokio::time::sleep(heartbeat_delay).await;
                    let ping = P2PMessage::Ping {
                        timestamp: unix_timestamp_secs(),
                    };
                    if let Err(e) = send_message(&mut stream, &ping).await {
                        tracing::debug!("[P2P] {} ping send error: {}", peer_addr, e);
                        break;
                    }

                    match tokio::time::timeout(Duration::from_secs(5), recv_message(&mut stream)).await {
                        Ok(Ok(P2PMessage::Pong { .. })) => {}
                        Ok(Ok(P2PMessage::Disconnect { reason })) => {
                            tracing::debug!("[P2P] {} disconnected: {}", peer_addr, reason);
                            break;
                        }
                        Ok(Ok(other)) => {
                            tracing::debug!("[P2P] {} unexpected keepalive reply: {}", peer_addr, other.label());
                        }
                        Ok(Err(e)) => {
                            tracing::debug!("[P2P] {} keepalive read error: {}", peer_addr, e);
                            break;
                        }
                        Err(_) => {
                            tracing::debug!("[P2P] {} keepalive timeout", peer_addr);
                            break;
                        }
                    }
                }

                peer_manager.set_state(&peer_addr, PeerState::Disconnected);
            }
            Err(e) => {
                tracing::debug!("[P2P] {} connect error: {}", peer_addr, e);
                peer_manager.set_state(&peer_addr, PeerState::Disconnected);
            }
        }

        tokio::time::sleep(reconnect_delay).await;
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

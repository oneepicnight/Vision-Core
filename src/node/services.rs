use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use crate::chain::{apply_block, AcceptResult, ChainState};
use crate::config::constants::{BLOCK_TARGET_TXS, MIN_PEERS_FOR_MINING, TARGET_OUTBOUND_PEERS};
use crate::config::settings::Settings;
use crate::mempool::Mempool;
use crate::miner::MinerManager;
use crate::node::recovery::RecoveryState;
use crate::p2p::connection::{recv_message, send_message, InboundBlock, P2PConnectionManager};
use crate::p2p::messages::P2PMessage;
use crate::p2p::peer_manager::{validate_dial_address, PeerManager, PeerState};
use crate::p2p::protocol::{validate_handshake, ChainSummary, HandshakeResult};
use crate::p2p::sync::SyncGuard;
use crate::types::transaction::canonical_tx_id;

/// Spawn all background services for a running node.
///
/// Each service runs in its own Tokio task. Services communicate via shared
/// Arc<Mutex<>> handles or Tokio channels.
pub async fn start_services(
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    mempool: Arc<Mempool>,
    miner_manager: Option<Arc<MinerManager>>,
    recovery_state: Arc<RecoveryState>,
    settings: &Settings,
) -> Result<()> {
    let p2p_addr: SocketAddr = settings.p2p_addr.parse()?;
    let (inbound_block_sender, inbound_block_receiver) = mpsc::channel(64);
    let (discovered_peer_sender, discovered_peer_receiver) = mpsc::channel(128);
    let conn_mgr = Arc::new(
        P2PConnectionManager::new_with_advertised(
            p2p_addr,
            chain.clone(),
            peer_manager.clone(),
            settings.p2p_advertised_host.clone(),
            settings.p2p_advertised_port,
            settings.allow_private_peer_addresses,
        )
        .with_block_receiver(inbound_block_sender)
        .with_peer_discovery(discovered_peer_sender.clone()),
    );
    let listener = conn_mgr.bind_listener().await?;

    {
        let chain_ref = chain.clone();
        let mempool_ref = mempool.clone();
        let conn_mgr_ref = conn_mgr.clone();
        let miner_manager_ref = miner_manager.clone();
        tokio::spawn(async move {
            import_announced_blocks(
                inbound_block_receiver,
                chain_ref,
                mempool_ref,
                conn_mgr_ref,
                miner_manager_ref,
            )
            .await;
        });
    }

    {
        let mgr = conn_mgr.clone();
        tokio::spawn(async move {
            if let Err(e) = mgr.run_listener(listener).await {
                tracing::error!("[P2P] Listener error: {}", e);
            }
        });
    }

    {
        let chain_ref = chain.clone();
        let peer_manager_ref = peer_manager.clone();
        let conn_mgr_ref = conn_mgr.clone();
        let mempool_ref = mempool.clone();
        let recovery_ref = recovery_state.clone();
        let discovery_sender_ref = discovered_peer_sender.clone();
        let allow_private = settings.allow_private_peer_addresses;
        tokio::spawn(async move {
            peer_dial_supervisor(
                discovered_peer_receiver,
                discovery_sender_ref,
                chain_ref,
                peer_manager_ref,
                conn_mgr_ref,
                mempool_ref,
                recovery_ref,
                allow_private,
            )
            .await;
        });
        for seed_peer in &settings.seed_peers {
            discovered_peer_sender
                .try_send(seed_peer.clone())
                .map_err(|_| anyhow::anyhow!("peer discovery queue unavailable at startup"))?;
        }
    }

    {
        let mgr = conn_mgr.clone();
        let pm = peer_manager.clone();
        let chain_ref = chain.clone();
        let mempool_ref = mempool.clone();
        let recovery_ref = recovery_state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(20));
            let mut sync_guard = crate::p2p::sync::SyncGuard::new();
            loop {
                interval.tick().await;
                if let Err(e) = crate::p2p::sync::watchdog_step(
                    &mgr,
                    &chain_ref,
                    &pm,
                    &mut sync_guard,
                    Some(mempool_ref.as_ref()),
                    Some(recovery_ref.as_ref()),
                )
                .await
                {
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
            let conn_mgr_ref = conn_mgr.clone();
            let miner_addr = settings.miner_address.clone();
            let recovery_ref = recovery_state.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(250));
                loop {
                    interval.tick().await;

                    if peer_manager_ref.connected_count() < MIN_PEERS_FOR_MINING {
                        miner_manager.clear_job();
                        continue;
                    }

                    if recovery_ref.should_pause_mining() {
                        miner_manager.clear_job();
                        tracing::debug!(
                            "[MINER] mining paused during {}",
                            recovery_ref.snapshot().state
                        );
                        continue;
                    }

                    let maybe_job = {
                        let chain_guard = chain_ref.lock().await;
                        let txs = mempool_ref.select_for_block(BLOCK_TARGET_TXS);
                        miner_manager.build_candidate_for_tip(&chain_guard, &miner_addr, txs)
                    };

                    let Some(job) = maybe_job else {
                        continue;
                    };

                    miner_manager.build_job(job.clone());
                    let job_id = job.job_id;
                    let miner_manager_ref = miner_manager.clone();

                    let mined = tokio::task::spawn_blocking(move || {
                        let mut nonce = 0u64;
                        loop {
                            if !miner_manager_ref.is_current_job(job_id) {
                                return None;
                            }
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
                    let announced_block = block.clone();
                    let confirmed_tx_ids: Vec<String> =
                        block.txs.iter().map(canonical_tx_id).collect();
                    let mut chain_guard = chain_ref.lock().await;
                    match miner_manager.submit_solution(&mut chain_guard, block) {
                        AcceptResult::CanonExtension { .. } => {
                            mempool_ref.remove_confirmed(&confirmed_tx_ids);
                            if let Some(recovery) = chain_guard.pending_reorg_recovery.take() {
                                let report =
                                    mempool_ref.requeue_after_reorg(&chain_guard, recovery);
                                tracing::info!(
                                    "[MEMPOOL] reorg recovery accepted={} rejected={}",
                                    report.accepted.len(),
                                    report.rejected.len()
                                );
                            }
                            tracing::info!(
                                "[MINER] accepted mined block and cleared {} txs",
                                confirmed_tx_ids.len()
                            );
                            let recipients = conn_mgr_ref.announce_block(&announced_block, None);
                            tracing::debug!(
                                "[P2P] announced locally mined block {} to {} sessions",
                                announced_block.hash(),
                                recipients
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

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn peer_dial_supervisor(
    mut candidates: mpsc::Receiver<String>,
    discovered_peer_sender: mpsc::Sender<String>,
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    conn_mgr: Arc<P2PConnectionManager>,
    mempool: Arc<Mempool>,
    recovery_state: Arc<RecoveryState>,
    allow_private: bool,
) {
    let mut scheduled = HashSet::new();
    while let Some(candidate) = candidates.recv().await {
        let peer_socket = match validate_dial_address(&candidate, allow_private) {
            Ok(peer_socket) => peer_socket,
            Err(error) => {
                tracing::debug!("[P2P] ignored discovered peer {}: {}", candidate, error);
                continue;
            }
        };
        let peer_addr = peer_socket.to_string();
        if conn_mgr.is_local_dial_address(peer_socket) {
            tracing::trace!("[P2P] ignored self peer candidate {}", peer_addr);
            continue;
        }
        if !scheduled.insert(peer_addr.clone()) {
            continue;
        }
        if scheduled.len() > TARGET_OUTBOUND_PEERS {
            scheduled.remove(&peer_addr);
            tracing::debug!(
                "[P2P] ignored peer {} after reaching outbound target {}",
                peer_addr,
                TARGET_OUTBOUND_PEERS
            );
            continue;
        }

        peer_manager.upsert(&peer_addr, true);
        let chain_ref = chain.clone();
        let peer_manager_ref = peer_manager.clone();
        let conn_mgr_ref = conn_mgr.clone();
        let mempool_ref = mempool.clone();
        let recovery_ref = recovery_state.clone();
        let discovery_sender_ref = discovered_peer_sender.clone();
        tokio::spawn(async move {
            seed_peer_loop(
                chain_ref,
                peer_manager_ref,
                conn_mgr_ref,
                mempool_ref,
                peer_addr,
                recovery_ref,
                discovery_sender_ref,
            )
            .await;
        });
    }
}

async fn import_announced_blocks(
    mut receiver: mpsc::Receiver<InboundBlock>,
    chain: Arc<Mutex<ChainState>>,
    mempool: Arc<Mempool>,
    conn_mgr: Arc<P2PConnectionManager>,
    miner_manager: Option<Arc<MinerManager>>,
) {
    while let Some(InboundBlock { block, peer_addr }) = receiver.recv().await {
        let block_hash = block.hash().to_string();
        let confirmed_tx_ids: Vec<String> = block.txs.iter().map(canonical_tx_id).collect();
        let result = {
            let mut chain_guard = chain.lock().await;
            let result = apply_block(&mut chain_guard, &block, Some(&peer_addr));
            if matches!(result, AcceptResult::CanonExtension { .. }) {
                chain_guard.refresh_cached_state_root_from_tip();
                mempool.remove_confirmed(&confirmed_tx_ids);
                if let Some(recovery) = chain_guard.pending_reorg_recovery.take() {
                    let report = mempool.requeue_after_reorg(&chain_guard, recovery);
                    tracing::info!(
                        "[MEMPOOL] announced-block reorg recovery accepted={} rejected={}",
                        report.accepted.len(),
                        report.rejected.len()
                    );
                }
            }
            result
        };

        match result {
            AcceptResult::CanonExtension { height } => {
                if let Some(miner_manager) = miner_manager.as_ref() {
                    miner_manager.clear_job();
                }
                let recipients = conn_mgr.announce_block(&block, Some(&peer_addr));
                tracing::info!(
                    "[P2P] accepted announced block height={} hash={} peer={} relayed_to={}",
                    height,
                    block_hash,
                    peer_addr,
                    recipients
                );
            }
            AcceptResult::SideChain { .. } => {
                tracing::debug!(
                    "[P2P] announced block {} from {} stored on side chain",
                    block_hash,
                    peer_addr
                );
            }
            AcceptResult::StoredOrphan { .. } => {
                tracing::debug!(
                    "[P2P] announced block {} from {} stored as orphan",
                    block_hash,
                    peer_addr
                );
            }
            AcceptResult::Rejected(reason) => {
                tracing::debug!(
                    "[P2P] announced block {} from {} rejected: {}",
                    block_hash,
                    peer_addr,
                    reason
                );
            }
        }
    }
}

async fn seed_peer_loop(
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    conn_mgr: Arc<P2PConnectionManager>,
    mempool: Arc<Mempool>,
    peer_addr: String,
    recovery_state: Arc<RecoveryState>,
    discovered_peer_sender: mpsc::Sender<String>,
) {
    let reconnect_delay = Duration::from_secs(2);
    let heartbeat_delay = Duration::from_secs(5);
    let mut sync_guard = SyncGuard::new();

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
                let local_hs = conn_mgr.local_handshake(local_height);

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
                        tracing::warn!(
                            "[P2P] {} unexpected reply during handshake: {}",
                            peer_addr,
                            other.label()
                        );
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
                        let local_height = chain.lock().await.current_height();
                        tracing::info!(
                            "[P2P] {} handshake complete local_height={} remote_height={}",
                            peer_addr,
                            local_height,
                            remote_hs.chain_height
                        );
                        for candidate in remote_hs.seed_peers.iter().take(32) {
                            let _ = discovered_peer_sender.try_send(candidate.clone());
                        }
                    }
                    other => {
                        let reason = match other {
                            HandshakeResult::VersionMismatch { remote, ours } => {
                                format!(
                                    "unsupported protocol version: remote={} ours={}",
                                    remote, ours
                                )
                            }
                            HandshakeResult::WrongChainId => "wrong chain identity".to_string(),
                            HandshakeResult::WrongGenesisHash => "wrong genesis hash".to_string(),
                            HandshakeResult::WrongEconHash => "wrong economic version".to_string(),
                            HandshakeResult::WrongPowParams => {
                                "wrong pow/consensus version".to_string()
                            }
                            HandshakeResult::SelfConnection => {
                                "self-connection rejected".to_string()
                            }
                            HandshakeResult::Accepted => "handshake accepted".to_string(),
                        };
                        let _ = send_message(&mut stream, &P2PMessage::Disconnect { reason }).await;
                        peer_manager.set_state(&peer_addr, PeerState::Disconnected);
                        tokio::time::sleep(reconnect_delay).await;
                        continue;
                    }
                }

                let sessions = conn_mgr.sessions();
                let (session_generation, mut session_receiver) =
                    sessions.register(peer_addr.clone());
                let mut requested_block_hash = None;

                'session: loop {
                    while let Ok(outbound) = session_receiver.try_recv() {
                        if let Err(error) = send_message(&mut stream, &outbound).await {
                            tracing::debug!(
                                "[P2P] {} outbound session send error: {}",
                                peer_addr,
                                error
                            );
                            break 'session;
                        }
                    }

                    let remote_summary = match poll_peer_height(
                        &mut stream,
                        &peer_addr,
                        &chain,
                        &peer_manager,
                        &conn_mgr,
                        &mut requested_block_hash,
                    )
                    .await
                    {
                        Ok(summary) => summary,
                        Err(e) => {
                            tracing::debug!("[P2P] {} height poll error: {}", peer_addr, e);
                            break;
                        }
                    };
                    let local_summary = {
                        let g = chain.lock().await;
                        ChainSummary::from_chain(&g)
                    };
                    let remote_has_more_work = remote_summary.cumulative_work
                        > local_summary.cumulative_work
                        && remote_summary.tip_hash != local_summary.tip_hash;
                    tracing::debug!(
                        "[SYNC] summary compare peer={} local_h={} local_work={} remote_h={} remote_work={} lag={} tip_diff={}",
                        peer_addr,
                        local_summary.height,
                        local_summary.cumulative_work,
                        remote_summary.height,
                        remote_summary.cumulative_work,
                        remote_summary.height.saturating_sub(local_summary.height),
                        remote_summary.tip_hash != local_summary.tip_hash
                    );
                    if remote_has_more_work {
                        recovery_state.begin_higher_work_recovery(
                            &peer_addr,
                            local_summary.clone(),
                            remote_summary.clone(),
                        );
                    }
                    if remote_summary.height > local_summary.height || remote_has_more_work {
                        let lag = remote_summary.height.saturating_sub(local_summary.height);
                        tracing::info!(
                            "[SYNC] peer candidate={} local_height={} local_work={} remote_height={} remote_work={} lag={} work_ahead={}",
                            peer_addr,
                            local_summary.height,
                            local_summary.cumulative_work,
                            remote_summary.height,
                            remote_summary.cumulative_work,
                            lag,
                            remote_has_more_work
                        );
                        if sync_guard.is_blocked() {
                            tracing::trace!(
                                "[SYNC] {} catch-up skipped (sync in progress or throttled)",
                                peer_addr
                            );
                        } else {
                            sync_guard.mark_started();
                            let result = crate::p2p::sync::live_sync_from_peer(
                                &conn_mgr,
                                &chain,
                                &peer_manager,
                                &peer_addr,
                                Some(mempool.as_ref()),
                            )
                            .await;
                            sync_guard.mark_done();
                            match result {
                                Ok(_) if remote_has_more_work => {
                                    let after = {
                                        let g = chain.lock().await;
                                        ChainSummary::from_chain(&g)
                                    };
                                    if after.cumulative_work >= remote_summary.cumulative_work {
                                        recovery_state
                                            .clear("higher-work branch adopted or no longer ahead");
                                    } else {
                                        recovery_state.mark_limited("higher-work recovery incomplete after seed-peer sync batch");
                                    }
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    let reason = e.to_string();
                                    if remote_has_more_work {
                                        if reason.contains("sync import rejected") {
                                            recovery_state
                                                .clear("advertised branch was locally rejected");
                                        } else {
                                            recovery_state.mark_high_risk(format!(
                                                "higher-work recovery failed: {}",
                                                reason
                                            ));
                                        }
                                    }
                                    tracing::warn!(
                                        "[SYNC] catch-up trigger error for {}: {}",
                                        peer_addr,
                                        e
                                    );
                                    break;
                                }
                            }
                        }
                    }

                    tokio::time::sleep(heartbeat_delay).await;
                    let ping = P2PMessage::Ping {
                        timestamp: unix_timestamp_secs(),
                    };
                    if let Err(e) = send_message(&mut stream, &ping).await {
                        tracing::debug!("[P2P] {} ping send error: {}", peer_addr, e);
                        break;
                    }

                    match tokio::time::timeout(Duration::from_secs(5), recv_message(&mut stream))
                        .await
                    {
                        Ok(Ok(P2PMessage::Pong { .. })) => {}
                        Ok(Ok(message)) => {
                            if let Err(error) = handle_seed_session_message(
                                &mut stream,
                                message,
                                &peer_addr,
                                &chain,
                                &conn_mgr,
                                &mut requested_block_hash,
                            )
                            .await
                            {
                                tracing::debug!(
                                    "[P2P] {} keepalive message error: {}",
                                    peer_addr,
                                    error
                                );
                                break;
                            }
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

                sessions.unregister(&peer_addr, session_generation);
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

async fn poll_peer_height(
    stream: &mut (impl tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + Unpin),
    peer_addr: &str,
    chain: &Arc<Mutex<ChainState>>,
    peer_manager: &Arc<PeerManager>,
    conn_mgr: &Arc<P2PConnectionManager>,
    requested_block_hash: &mut Option<String>,
) -> Result<ChainSummary> {
    peer_manager.record_height_poll_sent(peer_addr);
    send_message(stream, &P2PMessage::GetHeight).await?;
    let mut remote_summary = None;
    for _ in 0..8 {
        match tokio::time::timeout(Duration::from_secs(5), recv_message(stream)).await {
            Ok(Ok(P2PMessage::Height { summary })) => {
                remote_summary = Some(summary);
                break;
            }
            Ok(Ok(P2PMessage::GetHeight)) => {
                let summary = {
                    let g = chain.lock().await;
                    ChainSummary::from_chain(&g)
                };
                send_message(stream, &P2PMessage::Height { summary }).await?;
            }
            Ok(Ok(P2PMessage::Ping { timestamp })) => {
                send_message(stream, &P2PMessage::Pong { timestamp }).await?;
            }
            Ok(Ok(P2PMessage::Disconnect { reason })) => {
                return Err(anyhow::anyhow!("peer rejected height query: {}", reason))
            }
            Ok(Ok(other)) => {
                handle_seed_session_message(
                    stream,
                    other,
                    peer_addr,
                    chain,
                    conn_mgr,
                    requested_block_hash,
                )
                .await?;
            }
            Ok(Err(e)) => return Err(anyhow::anyhow!("height poll read error: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("height poll timeout for {}", peer_addr)),
        }
    }
    let remote_summary = remote_summary
        .ok_or_else(|| anyhow::anyhow!("height poll exceeded message limit for {}", peer_addr))?;
    peer_manager.note_peer_summary(peer_addr, remote_summary.clone(), false);
    let local_summary = {
        let g = chain.lock().await;
        ChainSummary::from_chain(&g)
    };
    tracing::debug!(
        "[P2P] {} learned remote summary height={} work={} tip={:?} local_height={} local_work={} local_tip={:?}",
        peer_addr,
        remote_summary.height,
        remote_summary.cumulative_work,
        remote_summary.tip_hash,
        local_summary.height,
        local_summary.cumulative_work,
        local_summary.tip_hash
    );
    Ok(remote_summary)
}

async fn handle_seed_session_message(
    stream: &mut (impl tokio::io::AsyncReadExt + tokio::io::AsyncWriteExt + Unpin),
    message: P2PMessage,
    peer_addr: &str,
    chain: &Arc<Mutex<ChainState>>,
    conn_mgr: &Arc<P2PConnectionManager>,
    requested_block_hash: &mut Option<String>,
) -> Result<()> {
    match message {
        P2PMessage::AnnounceBlock(announcement) => {
            if !is_canonical_hash(&announcement.hash) || !is_canonical_hash(&announcement.prev) {
                anyhow::bail!("peer sent malformed block announcement");
            }
            let known = chain
                .lock()
                .await
                .block_by_hash(&announcement.hash)
                .is_some();
            if !known && requested_block_hash.is_none() {
                *requested_block_hash = Some(announcement.hash.clone());
                send_message(
                    stream,
                    &P2PMessage::GetBlock {
                        hash: announcement.hash,
                    },
                )
                .await?;
            }
        }
        P2PMessage::GetBlock { hash } => {
            let block = chain.lock().await.block_by_hash(&hash);
            let Some(block) = block else {
                anyhow::bail!("peer requested unknown block {}", hash);
            };
            send_message(stream, &P2PMessage::Block { block }).await?;
        }
        P2PMessage::Block { block } => {
            let Some(expected_hash) = requested_block_hash.take() else {
                anyhow::bail!("peer sent an unrequested block");
            };
            let actual_hash = block.hash().to_string();
            if actual_hash != expected_hash {
                anyhow::bail!(
                    "peer returned block {} for request {}",
                    actual_hash,
                    expected_hash
                );
            }
            conn_mgr
                .queue_inbound_block(block, peer_addr.to_string())
                .await?;
        }
        P2PMessage::GetHeight => {
            let summary = {
                let chain_guard = chain.lock().await;
                ChainSummary::from_chain(&chain_guard)
            };
            send_message(stream, &P2PMessage::Height { summary }).await?;
        }
        P2PMessage::Ping { timestamp } => {
            send_message(stream, &P2PMessage::Pong { timestamp }).await?;
        }
        P2PMessage::Pong { .. } | P2PMessage::Height { .. } => {}
        P2PMessage::Disconnect { reason } => {
            anyhow::bail!("peer disconnected: {}", reason);
        }
        other => anyhow::bail!("unexpected session message: {}", other.label()),
    }
    Ok(())
}

fn is_canonical_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::Settings;
    use crate::genesis::genesis_block;
    use crate::p2p::messages::P2PMessage;
    use crate::p2p::peer_manager::PeerState;
    use std::net::TcpListener as StdTcpListener;
    use tokio::io::duplex;
    use tokio::task::JoinHandle;

    fn temp_chain() -> Arc<Mutex<ChainState>> {
        let db = sled::Config::new().temporary(true).open().unwrap();
        Arc::new(Mutex::new(ChainState::empty(db)))
    }

    fn temp_peer_manager() -> Arc<PeerManager> {
        Arc::new(PeerManager::new())
    }

    fn test_settings(p2p_addr: SocketAddr) -> Settings {
        Settings {
            data_dir: "./data".to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            p2p_addr: p2p_addr.to_string(),
            p2p_advertised_host: None,
            p2p_advertised_port: None,
            allow_private_peer_addresses: true,
            miner_address: "0".repeat(64),
            mining_enabled: false,
            mining_threads: 0,
            alpha_airdrop_enabled: false,
            seed_peers: Vec::new(),
        }
    }

    async fn scripted_height_peer(height: u64) -> (tokio::io::DuplexStream, JoinHandle<()>) {
        let (client, mut server) = duplex(4096);
        let handle = tokio::spawn(async move {
            match recv_message(&mut server).await.unwrap() {
                P2PMessage::GetHeight => {
                    send_message(
                        &mut server,
                        &P2PMessage::Height {
                            summary: ChainSummary::new(
                                height,
                                Some(format!("{:064x}", height)),
                                height as u128,
                            ),
                        },
                    )
                    .await
                    .unwrap();
                }
                other => panic!("expected height request, got {:?}", other),
            }
        });
        (client, handle)
    }

    #[tokio::test]
    async fn poll_peer_height_records_remote_height() {
        let chain = temp_chain();
        let pm = temp_peer_manager();
        let peer_addr = "127.0.0.1:19099";
        pm.upsert(peer_addr, true);
        pm.set_state(peer_addr, PeerState::Connected);
        let (mut client, handle) = scripted_height_peer(42).await;
        let conn_mgr = Arc::new(P2PConnectionManager::new(
            "127.0.0.1:0".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        ));
        let mut requested_block_hash = None;
        let remote_summary = poll_peer_height(
            &mut client,
            peer_addr,
            &chain,
            &pm,
            &conn_mgr,
            &mut requested_block_hash,
        )
        .await
        .unwrap();
        assert_eq!(remote_summary.height, 42);
        assert_eq!(remote_summary.cumulative_work, 42);
        assert_eq!(pm.best_remote_height(), 42);
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn poll_peer_height_times_out_when_peer_stalls() {
        let chain = temp_chain();
        let pm = temp_peer_manager();
        let peer_addr = "127.0.0.1:19110";
        pm.upsert(peer_addr, true);
        pm.set_state(peer_addr, PeerState::Connected);
        let (mut client, _server) = duplex(4096);
        let conn_mgr = Arc::new(P2PConnectionManager::new(
            "127.0.0.1:0".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        ));
        let mut requested_block_hash = None;
        let result = poll_peer_height(
            &mut client,
            peer_addr,
            &chain,
            &pm,
            &conn_mgr,
            &mut requested_block_hash,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(pm.best_remote_height(), 0);
    }

    #[tokio::test]
    async fn announced_block_uses_unified_import_and_relays_to_other_sessions() {
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let conn_mgr = Arc::new(P2PConnectionManager::new(
            "127.0.0.1:0".parse().unwrap(),
            chain.clone(),
            peer_manager,
        ));
        let sessions = conn_mgr.sessions();
        let (source_generation, mut source_messages) =
            sessions.register("127.0.0.1:19120".to_string());
        let (other_generation, mut other_messages) =
            sessions.register("127.0.0.1:19121".to_string());
        let (sender, receiver) = mpsc::channel(1);
        let importer = tokio::spawn(import_announced_blocks(
            receiver,
            chain.clone(),
            Arc::new(Mempool::new()),
            conn_mgr,
            None,
        ));

        let block = genesis_block();
        let block_hash = block.hash().to_string();
        sender
            .send(InboundBlock {
                block,
                peer_addr: "127.0.0.1:19120".to_string(),
            })
            .await
            .unwrap();

        let relayed = tokio::time::timeout(Duration::from_secs(2), other_messages.recv())
            .await
            .unwrap()
            .expect("accepted block should be relayed to another active session");
        match relayed {
            P2PMessage::AnnounceBlock(announcement) => assert_eq!(announcement.hash, block_hash),
            other => panic!("expected relayed block announcement, got {:?}", other),
        }
        assert!(source_messages.try_recv().is_err());
        assert!(chain.lock().await.block_by_hash(&block_hash).is_some());

        sessions.unregister("127.0.0.1:19120", source_generation);
        sessions.unregister("127.0.0.1:19121", other_generation);
        drop(sender);
        importer.await.unwrap();
    }

    #[tokio::test]
    async fn start_services_fails_when_p2p_listener_bind_fails() {
        let occupied_listener = StdTcpListener::bind(("0.0.0.0", 0)).unwrap();
        let occupied_addr = occupied_listener.local_addr().unwrap();
        let settings = test_settings(occupied_addr);

        let result = start_services(
            temp_chain(),
            temp_peer_manager(),
            Arc::new(Mempool::new()),
            None,
            Arc::new(RecoveryState::new()),
            &settings,
        )
        .await;

        assert!(
            result.is_err(),
            "occupied listener address should fail startup"
        );
    }
}

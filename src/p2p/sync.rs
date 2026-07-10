use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::chain::accept::{apply_block, AcceptResult};
use crate::chain::state::ChainState;
use crate::config::constants::{STALL_OVERRIDE_SECS, SYNC_CLEAR_JOB_MIN_LAG, SYNC_LAG_THRESHOLD, TARGET_BLOCK_TIME};
use crate::genesis::genesis_block;
use crate::p2p::connection::{recv_message, send_message, P2PConnectionManager};
use crate::p2p::messages::P2PMessage;
use crate::p2p::peer_manager::PeerManager;
use crate::p2p::protocol::{validate_handshake, HandshakeMessage, HandshakeResult};
use crate::types::Block;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDecision {
    Synced,
    Behind { peer_addr: String, lag: u64 },
}

pub fn should_sync(peer_manager: &PeerManager, local_height: u64) -> SyncDecision {
    let remote_height = peer_manager.best_remote_height();
    let lag = remote_height.saturating_sub(local_height);
    if lag < SYNC_LAG_THRESHOLD {
        return SyncDecision::Synced;
    }
    match peer_manager.best_sync_target(local_height) {
        Some(peer_addr) => SyncDecision::Behind { peer_addr, lag },
        None => SyncDecision::Synced,
    }
}

pub struct SyncGuard {
    in_progress: bool,
    cooldown_until: Option<Instant>,
}

impl SyncGuard {
    pub fn new() -> Self {
        Self { in_progress: false, cooldown_until: None }
    }
    pub fn is_in_progress(&self) -> bool { self.in_progress }
    pub fn is_throttled(&self) -> bool {
        self.cooldown_until.map(|t| Instant::now() < t).unwrap_or(false)
    }
    pub fn is_blocked(&self) -> bool { self.is_in_progress() || self.is_throttled() }
    pub fn mark_started(&mut self) { self.in_progress = true; }
    pub fn mark_done(&mut self) {
        self.in_progress = false;
        self.cooldown_until = Some(Instant::now() + Duration::from_secs(STALL_OVERRIDE_SECS));
    }
    #[cfg(test)]
    pub fn reset(&mut self) {
        self.in_progress = false;
        self.cooldown_until = None;
    }
}

impl Default for SyncGuard {
    fn default() -> Self { Self::new() }
}

async fn live_sync_from_peer(
    conn_mgr: &P2PConnectionManager,
    chain: &Arc<Mutex<ChainState>>,
    peer_manager: &PeerManager,
    peer_addr: &str,
) -> Result<usize> {
    let peer_socket: SocketAddr = peer_addr.parse()?;
    let mut stream = P2PConnectionManager::connect(peer_socket).await?;
    let local_nonce = conn_mgr.local_node_nonce();
    let local_height = chain.lock().await.current_height();
    let remote_height = peer_manager.best_remote_height();
    tracing::debug!("[SYNC] height snapshot local={} remote={}", local_height, remote_height);

    send_message(&mut stream, &P2PMessage::Handshake(HandshakeMessage::new(local_height, local_nonce))).await?;
    let remote_hs = match recv_message(&mut stream).await? {
        P2PMessage::Handshake(hs) => hs,
        P2PMessage::Disconnect { reason } => return Err(anyhow!("peer rejected sync handshake: {}", reason)),
        other => return Err(anyhow!("unexpected handshake reply: {}", other.label())),
    };
    match validate_handshake(&remote_hs, local_nonce) {
        HandshakeResult::Accepted => {}
        other => return Err(anyhow!("sync handshake rejected: {:?}", other)),
    }
    peer_manager.note_peer_height(peer_addr, remote_hs.chain_height, false);
    tracing::info!("[SYNC] starting catchup from {} handshake remote_height={} local_height={}", peer_addr, remote_hs.chain_height, local_height);

    send_message(&mut stream, &P2PMessage::GetHeight).await?;
    let (remote_height, remote_tip_hash) = match recv_message(&mut stream).await? {
        P2PMessage::Height { height, tip_hash } => (height, tip_hash),
        P2PMessage::Disconnect { reason } => return Err(anyhow!("peer rejected height query: {}", reason)),
        other => return Err(anyhow!("unexpected height reply: {}", other.label())),
    };
    peer_manager.note_peer_height(peer_addr, remote_height, false);

    if remote_height <= local_height {
        tracing::debug!("[SYNC] {} already synced local_height={} remote_height={}", peer_addr, local_height, remote_height);
        return Ok(0);
    }

    tracing::info!("[SYNC] requesting block range {}..={} from {}", local_height + 1, remote_height, peer_addr);
    let mut current_hash = remote_tip_hash.ok_or_else(|| anyhow!("peer reported no tip hash at height {}", remote_height))?;
    let mut fetched = Vec::new();
    let mut seen = HashSet::new();

    loop {
        let known = { chain.lock().await.block_by_hash(&current_hash).is_some() };
        if known {
            break;
        }
        if !seen.insert(current_hash.clone()) {
            return Err(anyhow!("sync loop detected at {}", current_hash));
        }

        send_message(&mut stream, &P2PMessage::GetBlock { hash: current_hash.clone() }).await?;
        let block = match recv_message(&mut stream).await? {
            P2PMessage::Block { block } => block,
            P2PMessage::Disconnect { reason } => return Err(anyhow!("peer rejected block request: {}", reason)),
            other => return Err(anyhow!("unexpected block reply: {}", other.label())),
        };

        if block.hash() != current_hash {
            return Err(anyhow!("requested block {} but received {}", current_hash, block.hash()));
        }

        let parent_hash = block.header.parent_hash.clone();
        let is_genesis = block.header.number == 0;
        fetched.push(block);
        if is_genesis {
            break;
        }
        current_hash = parent_hash;
    }

    fetched.reverse();
    tracing::info!("[SYNC] received {} blocks from {}", fetched.len(), peer_addr);
    let mut imported = 0usize;
    for block in fetched {
        let result = {
            let mut g = chain.lock().await;
            apply_block(&mut g, &block, Some(peer_addr))
        };
        match result {
            AcceptResult::CanonExtension { .. } | AcceptResult::SideChain { .. } => {
                tracing::debug!("[SYNC] imported block height={} hash={}", block.header.number, block.hash());
                imported += 1;
            }
            AcceptResult::StoredOrphan { block_hash } => {
                return Err(anyhow!("sync import stored orphan {}", block_hash));
            }
            AcceptResult::Rejected(reason) => {
                return Err(anyhow!("sync import rejected: {}", reason));
            }
        }
    }

    tracing::info!("[SYNC] imported {} blocks from {}", imported, peer_addr);
    Ok(imported)
}

pub async fn watchdog_step(
    conn_mgr: &P2PConnectionManager,
    chain: &Arc<Mutex<ChainState>>,
    peer_manager: &PeerManager,
    guard: &mut SyncGuard,
) -> Result<()> {
    if guard.is_blocked() {
        tracing::trace!("[SYNC] watchdog skipped (sync in progress or throttled)");
        return Ok(());
    }

    let local_height = chain.lock().await.current_height();
    let remote_height = peer_manager.best_remote_height();
    tracing::debug!("[SYNC] height snapshot local={} remote={}", local_height, remote_height);
    match should_sync(peer_manager, local_height) {
        SyncDecision::Synced => {
            tracing::trace!("[SYNC] up to date (local h={})", local_height);
        }
        SyncDecision::Behind { peer_addr, lag } => {
            tracing::info!("[SYNC] starting catchup from {} lag={} local h={} remote h={}", peer_addr, lag, local_height, remote_height);
            if lag >= SYNC_CLEAR_JOB_MIN_LAG {
                tracing::info!("[SYNC] clearing miner job (lag={})", lag);
            }
            guard.mark_started();
            let result = live_sync_from_peer(conn_mgr, chain, peer_manager, &peer_addr).await;
            guard.mark_done();
            result?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::{apply_block, tests_helpers::make_test_block};
    use crate::p2p::peer_manager::PeerState;
    use tokio::time::{timeout, Duration};

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn pm_with(peers: &[(&str, u64)]) -> PeerManager {
        let pm = PeerManager::new();
        for &(addr, height) in peers {
            pm.upsert(addr, true);
            pm.set_state(addr, PeerState::Connected);
            pm.note_peer_height(addr, height, false);
        }
        pm
    }

    fn build_blocks(total_height: u64, bad_height: Option<u64>) -> Vec<Block> {
        let gen = genesis_block();
        let mut blocks = Vec::new();
        let mut parent_hash = gen.hash().to_string();
        let mut timestamp = gen.header.timestamp;
        for height in 1..=total_height {
            timestamp += TARGET_BLOCK_TIME;
            let mut block = make_test_block(&parent_hash, height, timestamp, 0xA0u8.wrapping_add(height as u8));
            if bad_height == Some(height) {
                block.header.state_root = "ff".repeat(32);
            }
            parent_hash = block.hash().to_string();
            blocks.push(block);
        }
        blocks
    }

    #[derive(Clone)]
    enum BlockReply {
        Matching,
        Specific(Block),
        MalformedFrame,
        Disconnect,
    }
    async fn spawn_scripted_peer(
        blocks: Vec<Block>,
        replies: Vec<BlockReply>,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let server_nonce = 0xDEADBEEF_u64;
            let client_hs = match recv_message(&mut stream).await.unwrap() {
                P2PMessage::Handshake(hs) => hs,
                other => panic!("expected handshake, got {:?}", other),
            };
            assert_eq!(validate_handshake(&client_hs, server_nonce), HandshakeResult::Accepted);
            let tip_height = blocks.last().map(|b| b.header.number).unwrap_or(0);
            let tip_hash = blocks.last().map(|b| b.hash().to_string());
            send_message(&mut stream, &P2PMessage::Handshake(HandshakeMessage::new(tip_height, server_nonce))).await.unwrap();

            let mut by_hash = HashMap::new();
            for block in blocks {
                by_hash.insert(block.hash().to_string(), block);
            }
            let mut replies = VecDeque::from(replies);

            loop {
                match recv_message(&mut stream).await {
                    Ok(P2PMessage::GetHeight) => {
                        send_message(&mut stream, &P2PMessage::Height { height: tip_height, tip_hash: tip_hash.clone() }).await.unwrap();
                    }
                    Ok(P2PMessage::GetBlock { hash }) => {
                        match replies.pop_front().unwrap_or(BlockReply::Matching) {
                            BlockReply::Matching => {
                                if let Some(block) = by_hash.get(&hash).cloned() {
                                    send_message(&mut stream, &P2PMessage::Block { block }).await.unwrap();
                                } else {
                                    break;
                                }
                            }
                            BlockReply::Specific(block) => {
                                send_message(&mut stream, &P2PMessage::Block { block }).await.unwrap();
                            }
                            BlockReply::MalformedFrame => {
                                stream.write_all(&4u32.to_be_bytes()).await.unwrap();
                                stream.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).await.unwrap();
                                break;
                            }
                            BlockReply::Disconnect => {
                                break;
                            }
                        }
                    }
                    Ok(P2PMessage::Disconnect { .. }) | Err(_) => break,
                    Ok(other) => panic!("unexpected message {:?}", other),
                }
            }
        });
        (addr, handle)
    }

    async fn spawn_mock_peer(blocks: Vec<Block>) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        spawn_scripted_peer(blocks, vec![]).await
    }

    fn seeded_chain(blocks: &[Block]) -> ChainState {
        let mut local_chain = temp_state();
        let gen = genesis_block();
        assert!(matches!(apply_block(&mut local_chain, &gen, None), AcceptResult::CanonExtension { height: 0 }));
        assert!(matches!(apply_block(&mut local_chain, &blocks[0], None), AcceptResult::CanonExtension { height: 1 }));
        local_chain
    }

    #[test]
    fn synced_when_no_peers() {
        let pm = PeerManager::new();
        assert_eq!(should_sync(&pm, 0), SyncDecision::Synced);
    }

    #[test]
    fn synced_when_lag_is_below_threshold() {
        let pm = pm_with(&[("a:9000", 104)]);
        assert_eq!(should_sync(&pm, 100), SyncDecision::Synced);
    }

    #[test]
    fn behind_returns_correct_peer_and_lag() {
        let pm = pm_with(&[("a:9000", 50), ("b:9000", 200), ("c:9000", 80)]);
        match should_sync(&pm, 0) {
            SyncDecision::Behind { peer_addr, lag } => {
                assert_eq!(peer_addr, "b:9000");
                assert_eq!(lag, 200);
            }
            other => panic!("expected Behind, got {:?}", other),
        }
    }

    #[test]
    fn guard_initially_not_blocked() {
        let g = SyncGuard::new();
        assert!(!g.is_in_progress());
        assert!(!g.is_throttled());
        assert!(!g.is_blocked());
    }

    #[test]
    fn guard_blocks_while_in_progress() {
        let mut g = SyncGuard::new();
        g.mark_started();
        assert!(g.is_blocked());
    }

    #[tokio::test]
    async fn watchdog_sync_imports_missing_blocks_over_tcp() {
        let remote_blocks = build_blocks(6, None);
        let (peer_addr, peer_task) = spawn_mock_peer(remote_blocks.clone()).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19101".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap().unwrap();

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 6);
        assert_eq!(g.tip_hash(), remote_blocks.last().unwrap().hash());
        drop(g);

        peer_task.await.unwrap();
        assert!(guard.is_throttled());
    }

    #[tokio::test]
    async fn watchdog_rejects_invalid_block_and_leaves_tip_unchanged() {
        let remote_blocks = build_blocks(6, Some(2));
        let (peer_addr, peer_task) = spawn_mock_peer(remote_blocks.clone()).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19102".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_rejects_invalid_pow_block_and_leaves_tip_unchanged() {
        let mut remote_blocks = build_blocks(6, None);
        remote_blocks[1].header.pow_hash = "ff".repeat(32);
        let (peer_addr, peer_task) = spawn_mock_peer(remote_blocks.clone()).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19103".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_rejects_invalid_state_root_block_and_leaves_tip_unchanged() {
        let remote_blocks = build_blocks(6, Some(2));
        let (peer_addr, peer_task) = spawn_mock_peer(remote_blocks.clone()).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19104".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_rejects_invalid_tx_root_block_and_leaves_tip_unchanged() {
        let mut remote_blocks = build_blocks(6, None);
        remote_blocks[1].header.tx_root = "00".repeat(32);
        let (peer_addr, peer_task) = spawn_mock_peer(remote_blocks.clone()).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19105".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_rejects_malformed_block_frame_and_leaves_tip_unchanged() {
        let remote_blocks = build_blocks(6, None);
        let (peer_addr, peer_task) = spawn_scripted_peer(remote_blocks.clone(), vec![BlockReply::MalformedFrame]).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19106".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_rejects_duplicate_block_replay_and_leaves_tip_unchanged() {
        let remote_blocks = build_blocks(6, None);
        let replay_block = remote_blocks[5].clone();
        let (peer_addr, peer_task) = spawn_scripted_peer(
            remote_blocks.clone(),
            vec![BlockReply::Matching, BlockReply::Specific(replay_block)],
        )
        .await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19107".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_handles_disconnect_during_block_transfer() {
        let remote_blocks = build_blocks(6, None);
        let (peer_addr, peer_task) = spawn_scripted_peer(remote_blocks.clone(), vec![BlockReply::Disconnect]).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19108".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();
        let result = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(result.is_err());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), remote_blocks[0].hash());
        drop(g);

        peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_recovers_with_valid_peer_after_malicious_peer_fails() {
        let mut malicious_blocks = build_blocks(6, None);
        malicious_blocks[1].header.state_root = "ff".repeat(32);
        let (malicious_addr, malicious_task) = spawn_mock_peer(malicious_blocks.clone()).await;
        let valid_blocks = build_blocks(6, None);
        let (valid_addr, valid_task) = spawn_mock_peer(valid_blocks.clone()).await;

        let local_chain = seeded_chain(&valid_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());

        let malicious_peer = malicious_addr.to_string();
        pm.upsert(&malicious_peer, true);
        pm.set_state(&malicious_peer, PeerState::Connected);
        pm.note_peer_height(&malicious_peer, 6, false);

        let valid_peer = valid_addr.to_string();
        pm.upsert(&valid_peer, true);
        pm.set_state(&valid_peer, PeerState::Connected);
        pm.note_peer_height(&valid_peer, 6, false);

        let conn_mgr = P2PConnectionManager::new("127.0.0.1:19109".parse().unwrap(), chain.clone(), pm.clone());
        let mut guard = SyncGuard::new();

        let first = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(first.is_err());

        guard.reset();
        pm.set_state(&malicious_peer, PeerState::Disconnected);
        pm.note_peer_height(&valid_peer, 6, false);

        let second = timeout(Duration::from_secs(5), watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard)).await.unwrap();
        assert!(second.is_ok());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 6);
        assert_eq!(g.tip_hash(), valid_blocks.last().unwrap().hash());
        drop(g);

        malicious_task.await.unwrap();
        valid_task.await.unwrap();
    }
}






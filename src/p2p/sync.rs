use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use tokio::sync::Mutex;

use crate::chain::accept::{apply_block, AcceptResult};
use crate::chain::state::ChainState;
use crate::config::constants::{STALL_OVERRIDE_SECS, SYNC_CLEAR_JOB_MIN_LAG, SYNC_LAG_THRESHOLD};
use crate::mempool::Mempool;
use crate::node::recovery::RecoveryState;
use crate::p2p::connection::{recv_message, send_message, P2PConnectionManager};
use crate::p2p::messages::P2PMessage;
use crate::p2p::peer_manager::PeerManager;
use crate::p2p::protocol::{validate_handshake, ChainSummary, HandshakeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDecision {
    Synced,
    Behind {
        peer_addr: String,
        lag: u64,
    },
    HigherWork {
        peer_addr: String,
        remote_work: u128,
        local_work: u128,
    },
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

pub fn should_sync_for_summary(peer_manager: &PeerManager, local: &ChainSummary) -> SyncDecision {
    let remote_height = peer_manager.best_remote_height();
    let lag = remote_height.saturating_sub(local.height);
    if lag >= SYNC_LAG_THRESHOLD {
        if let Some(peer_addr) = peer_manager.best_sync_target(local.height) {
            return SyncDecision::Behind { peer_addr, lag };
        }
    }

    if let Some(local_tip) = local.tip_hash.as_deref() {
        if let Some(peer_addr) =
            peer_manager.best_work_sync_target(local_tip, local.cumulative_work)
        {
            if let Some(remote) = peer_manager.peer_summary(&peer_addr) {
                return SyncDecision::HigherWork {
                    peer_addr,
                    remote_work: remote.cumulative_work,
                    local_work: local.cumulative_work,
                };
            }
        }
    }

    SyncDecision::Synced
}
pub struct SyncGuard {
    in_progress: bool,
    cooldown_until: Option<Instant>,
}

impl SyncGuard {
    pub fn new() -> Self {
        Self {
            in_progress: false,
            cooldown_until: None,
        }
    }
    pub fn is_in_progress(&self) -> bool {
        self.in_progress
    }
    pub fn is_throttled(&self) -> bool {
        self.cooldown_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }
    pub fn is_blocked(&self) -> bool {
        self.is_in_progress() || self.is_throttled()
    }
    pub fn mark_started(&mut self) {
        self.in_progress = true;
    }
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
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) async fn live_sync_from_peer(
    conn_mgr: &P2PConnectionManager,
    chain: &Arc<Mutex<ChainState>>,
    peer_manager: &PeerManager,
    peer_addr: &str,
    mempool: Option<&Mempool>,
) -> Result<usize> {
    let peer_socket: SocketAddr = peer_addr.parse()?;
    let mut stream = P2PConnectionManager::connect(peer_socket).await?;
    let local_nonce = conn_mgr.local_node_nonce();
    let local_height = chain.lock().await.current_height();
    let remote_height = peer_manager.best_remote_height();
    tracing::debug!(
        "[SYNC] height snapshot local={} remote={}",
        local_height,
        remote_height
    );

    send_message(
        &mut stream,
        &P2PMessage::Handshake(conn_mgr.local_handshake(local_height)),
    )
    .await?;
    let remote_hs = match recv_message(&mut stream).await? {
        P2PMessage::Handshake(hs) => hs,
        P2PMessage::Disconnect { reason } => {
            return Err(anyhow!("peer rejected sync handshake: {}", reason))
        }
        other => return Err(anyhow!("unexpected handshake reply: {}", other.label())),
    };
    match validate_handshake(&remote_hs, local_nonce) {
        HandshakeResult::Accepted => {}
        other => return Err(anyhow!("sync handshake rejected: {:?}", other)),
    }
    peer_manager.note_peer_height(peer_addr, remote_hs.chain_height, false);
    tracing::info!(
        "[SYNC] starting catchup from {} handshake remote_height={} local_height={}",
        peer_addr,
        remote_hs.chain_height,
        local_height
    );

    send_message(&mut stream, &P2PMessage::GetHeight).await?;
    let mut remote_summary = None;
    for _ in 0..8 {
        match tokio::time::timeout(Duration::from_secs(5), recv_message(&mut stream)).await?? {
            P2PMessage::Height { summary } => {
                remote_summary = Some(summary);
                break;
            }
            P2PMessage::GetHeight => {
                let summary = {
                    let g = chain.lock().await;
                    ChainSummary::from_chain(&g)
                };
                send_message(&mut stream, &P2PMessage::Height { summary }).await?;
            }
            P2PMessage::Ping { timestamp } => {
                send_message(&mut stream, &P2PMessage::Pong { timestamp }).await?;
            }
            P2PMessage::Disconnect { reason } => {
                return Err(anyhow!("peer rejected height query: {}", reason))
            }
            other => return Err(anyhow!("unexpected height reply: {}", other.label())),
        }
    }
    let remote_summary = remote_summary
        .ok_or_else(|| anyhow!("height query exceeded message limit for {}", peer_addr))?;
    peer_manager.note_peer_summary(peer_addr, remote_summary.clone(), false);

    let local_summary = {
        let g = chain.lock().await;
        ChainSummary::from_chain(&g)
    };
    let remote_height_ahead = remote_summary.height > local_summary.height;
    let remote_work_ahead = remote_summary.cumulative_work > local_summary.cumulative_work
        && remote_summary.tip_hash != local_summary.tip_hash;

    if !remote_height_ahead && !remote_work_ahead {
        tracing::debug!(
            "[SYNC] {} already synced local_h={} local_work={} remote_h={} remote_work={} tip_diff={}",
            peer_addr,
            local_summary.height,
            local_summary.cumulative_work,
            remote_summary.height,
            remote_summary.cumulative_work,
            remote_summary.tip_hash != local_summary.tip_hash
        );
        return Ok(0);
    }

    tracing::info!(
        "[SYNC] requesting branch from {} remote_h={} remote_work={} local_h={} local_work={} work_ahead={}",
        peer_addr,
        remote_summary.height,
        remote_summary.cumulative_work,
        local_summary.height,
        local_summary.cumulative_work,
        remote_work_ahead
    );
    let mut current_hash = remote_summary.tip_hash.clone().ok_or_else(|| {
        anyhow!(
            "peer reported no tip hash at height {}",
            remote_summary.height
        )
    })?;
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

        send_message(
            &mut stream,
            &P2PMessage::GetBlock {
                hash: current_hash.clone(),
            },
        )
        .await?;
        let block = match recv_message(&mut stream).await? {
            P2PMessage::Block { block } => block,
            P2PMessage::Disconnect { reason } => {
                return Err(anyhow!("peer rejected block request: {}", reason))
            }
            other => return Err(anyhow!("unexpected block reply: {}", other.label())),
        };

        if block.hash() != current_hash {
            return Err(anyhow!(
                "requested block {} but received {}",
                current_hash,
                block.hash()
            ));
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
    if let (Some(first), Some(last)) = (fetched.first(), fetched.last()) {
        tracing::info!(
            "[SYNC] received {} blocks from {} range={}..={} tip={}",
            fetched.len(),
            peer_addr,
            first.header.number,
            last.header.number,
            last.hash()
        );
    } else {
        tracing::info!("[SYNC] received 0 blocks from {}", peer_addr);
    }
    import_fetched_blocks(chain, mempool, peer_addr, fetched).await
}

async fn import_fetched_blocks(
    chain: &Arc<Mutex<ChainState>>,
    mempool: Option<&Mempool>,
    peer_addr: &str,
    fetched: Vec<crate::types::Block>,
) -> Result<usize> {
    let mut imported = 0usize;
    for block in fetched {
        let block_hash = block.hash().to_string();
        let result = {
            let mut g = chain.lock().await;
            if g.block_by_hash(&block_hash).is_some() {
                tracing::debug!(
                    "[SYNC] skipped already-known block height={} hash={}",
                    block.header.number,
                    block_hash
                );
                continue;
            }
            apply_block(&mut g, &block, Some(peer_addr))
        };
        match result {
            AcceptResult::CanonExtension { .. } => {
                {
                    let mut g = chain.lock().await;
                    g.refresh_cached_state_root_from_tip();
                    if let (Some(mempool), Some(recovery)) =
                        (mempool, g.pending_reorg_recovery.take())
                    {
                        let report = mempool.requeue_after_reorg(&g, recovery);
                        tracing::info!(
                            "[MEMPOOL] reorg recovery accepted={} rejected={}",
                            report.accepted.len(),
                            report.rejected.len()
                        );
                    }
                }
                tracing::debug!(
                    "[SYNC] imported block height={} hash={}",
                    block.header.number,
                    block_hash
                );
                imported += 1;
            }
            AcceptResult::SideChain { .. } => {
                tracing::debug!(
                    "[SYNC] imported side-chain block height={} hash={}",
                    block.header.number,
                    block_hash
                );
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
    mempool: Option<&Mempool>,
    recovery_state: Option<&RecoveryState>,
) -> Result<()> {
    if guard.is_blocked() {
        tracing::trace!("[SYNC] watchdog skipped (sync in progress or throttled)");
        return Ok(());
    }

    let local_summary = {
        let g = chain.lock().await;
        ChainSummary::from_chain(&g)
    };
    let remote_height = peer_manager.best_remote_height();
    tracing::debug!(
        "[SYNC] height snapshot local={} remote={} local_work={}",
        local_summary.height,
        remote_height,
        local_summary.cumulative_work
    );
    match should_sync_for_summary(peer_manager, &local_summary) {
        SyncDecision::Synced => {
            tracing::trace!("[SYNC] up to date (local h={})", local_summary.height);
        }
        SyncDecision::Behind { peer_addr, lag } => {
            tracing::info!(
                "[SYNC] starting catchup from {} lag={} local h={} remote h={}",
                peer_addr,
                lag,
                local_summary.height,
                remote_height
            );
            if lag >= SYNC_CLEAR_JOB_MIN_LAG {
                tracing::info!("[SYNC] clearing miner job (lag={})", lag);
            }
            guard.mark_started();
            let result =
                live_sync_from_peer(conn_mgr, chain, peer_manager, &peer_addr, mempool).await;
            guard.mark_done();
            result?;
        }
        SyncDecision::HigherWork {
            peer_addr,
            remote_work,
            local_work,
        } => {
            tracing::info!(
                "[SYNC] starting work-aware fork discovery from {} local_work={} remote_work={} local_h={}",
                peer_addr,
                local_work,
                remote_work,
                local_summary.height
            );
            let remote_summary = peer_manager.peer_summary(&peer_addr);
            if let (Some(recovery_state), Some(remote_summary)) =
                (recovery_state, remote_summary.clone())
            {
                recovery_state.begin_higher_work_recovery(
                    &peer_addr,
                    local_summary.clone(),
                    remote_summary,
                );
            }
            guard.mark_started();
            let result =
                live_sync_from_peer(conn_mgr, chain, peer_manager, &peer_addr, mempool).await;
            guard.mark_done();
            match result {
                Ok(_) => {
                    if let Some(recovery_state) = recovery_state {
                        let after = {
                            let g = chain.lock().await;
                            ChainSummary::from_chain(&g)
                        };
                        if after.cumulative_work >= remote_work {
                            recovery_state.clear("higher-work branch adopted or no longer ahead");
                        } else {
                            recovery_state
                                .mark_limited("higher-work recovery incomplete after sync batch");
                        }
                    }
                }
                Err(err) => {
                    if let Some(recovery_state) = recovery_state {
                        let reason = err.to_string();
                        if reason.contains("sync import rejected") {
                            recovery_state.clear("advertised branch was locally rejected");
                        } else {
                            recovery_state
                                .mark_high_risk(format!("higher-work recovery failed: {}", reason));
                        }
                    }
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use super::*;
    use crate::chain::accept::{apply_block, tests_helpers::make_test_block};
    use crate::config::constants::{DIFFICULTY_FLOOR, TARGET_BLOCK_TIME};
    use crate::genesis::genesis_block;
    use crate::p2p::peer_manager::PeerState;
    use crate::p2p::protocol::HandshakeMessage;
    use crate::types::Block;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
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
            let mut block = make_test_block(
                &parent_hash,
                height,
                timestamp,
                0xA0u8.wrapping_add(height as u8),
            );
            if bad_height == Some(height) {
                block.header.state_root = "ff".repeat(32);
            }
            parent_hash = block.hash().to_string();
            blocks.push(block);
        }
        blocks
    }

    fn build_alt_blocks(total_height: u64) -> Vec<Block> {
        let gen = genesis_block();
        let mut blocks = Vec::new();
        let mut parent_hash = gen.hash().to_string();
        let mut timestamp = gen.header.timestamp;
        for height in 1..=total_height {
            timestamp += TARGET_BLOCK_TIME;
            let block = make_test_block(
                &parent_hash,
                height,
                timestamp,
                0xC0u8.wrapping_add(height as u8),
            );
            parent_hash = block.hash().to_string();
            blocks.push(block);
        }
        blocks
    }

    fn summary_for_blocks(blocks: &[Block], advertised_work: Option<u128>) -> ChainSummary {
        let height = blocks.last().map(|b| b.header.number).unwrap_or(0);
        let tip_hash = blocks.last().map(|b| b.hash().to_string());
        let actual_work = DIFFICULTY_FLOOR as u128
            + blocks
                .iter()
                .map(|b| b.header.difficulty as u128)
                .sum::<u128>();
        ChainSummary::new(height, tip_hash, advertised_work.unwrap_or(actual_work))
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
        spawn_scripted_peer_on(listener, blocks, replies)
    }

    fn spawn_scripted_peer_on(
        listener: TcpListener,
        blocks: Vec<Block>,
        replies: Vec<BlockReply>,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let server_nonce = 0xDEADBEEF_u64;
            let client_hs = match recv_message(&mut stream).await.unwrap() {
                P2PMessage::Handshake(hs) => hs,
                other => panic!("expected handshake, got {:?}", other),
            };
            assert_eq!(
                validate_handshake(&client_hs, server_nonce),
                HandshakeResult::Accepted
            );
            let tip_height = blocks.last().map(|b| b.header.number).unwrap_or(0);
            let summary = summary_for_blocks(&blocks, None);
            send_message(
                &mut stream,
                &P2PMessage::Handshake(HandshakeMessage::new(tip_height, server_nonce)),
            )
            .await
            .unwrap();

            let mut by_hash = HashMap::new();
            for block in blocks {
                by_hash.insert(block.hash().to_string(), block);
            }
            let mut replies = VecDeque::from(replies);

            loop {
                match recv_message(&mut stream).await {
                    Ok(P2PMessage::GetHeight) => {
                        send_message(
                            &mut stream,
                            &P2PMessage::Height {
                                summary: summary.clone(),
                            },
                        )
                        .await
                        .unwrap();
                    }
                    Ok(P2PMessage::GetBlock { hash }) => {
                        match replies.pop_front().unwrap_or(BlockReply::Matching) {
                            BlockReply::Matching => {
                                if let Some(block) = by_hash.get(&hash).cloned() {
                                    send_message(&mut stream, &P2PMessage::Block { block })
                                        .await
                                        .unwrap();
                                } else {
                                    break;
                                }
                            }
                            BlockReply::Specific(block) => {
                                send_message(&mut stream, &P2PMessage::Block { block })
                                    .await
                                    .unwrap();
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

    async fn spawn_recording_peer(
        blocks: Vec<Block>,
        requests: Arc<Mutex<Vec<String>>>,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        spawn_recording_peer_with_work(blocks, requests, None).await
    }

    async fn spawn_recording_peer_with_work(
        blocks: Vec<Block>,
        requests: Arc<Mutex<Vec<String>>>,
        advertised_work: Option<u128>,
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
            assert_eq!(
                validate_handshake(&client_hs, server_nonce),
                HandshakeResult::Accepted
            );
            let tip_height = blocks.last().map(|b| b.header.number).unwrap_or(0);
            let summary = summary_for_blocks(&blocks, advertised_work);
            send_message(
                &mut stream,
                &P2PMessage::Handshake(HandshakeMessage::new(tip_height, server_nonce)),
            )
            .await
            .unwrap();

            let mut by_hash = HashMap::new();
            for block in blocks {
                by_hash.insert(block.hash().to_string(), block);
            }

            loop {
                match recv_message(&mut stream).await {
                    Ok(P2PMessage::GetHeight) => {
                        send_message(
                            &mut stream,
                            &P2PMessage::Height {
                                summary: summary.clone(),
                            },
                        )
                        .await
                        .unwrap();
                    }
                    Ok(P2PMessage::GetBlock { hash }) => {
                        requests.lock().await.push(hash.clone());
                        if let Some(block) = by_hash.get(&hash).cloned() {
                            send_message(&mut stream, &P2PMessage::Block { block })
                                .await
                                .unwrap();
                        } else {
                            break;
                        }
                    }
                    Ok(P2PMessage::Disconnect { .. }) | Err(_) => break,
                    Ok(other) => panic!("unexpected message {:?}", other),
                }
            }
        });
        (addr, handle)
    }

    fn seeded_chain(blocks: &[Block]) -> ChainState {
        let mut local_chain = temp_state();
        let gen = genesis_block();
        assert!(matches!(
            apply_block(&mut local_chain, &gen, None),
            AcceptResult::CanonExtension { height: 0 }
        ));
        assert!(matches!(
            apply_block(&mut local_chain, &blocks[0], None),
            AcceptResult::CanonExtension { height: 1 }
        ));
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

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19101".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap()
        .unwrap();

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 6);
        assert_eq!(g.tip_hash(), remote_blocks.last().unwrap().hash());
        drop(g);

        peer_task.await.unwrap();
        assert!(guard.is_throttled());
    }

    #[tokio::test]
    async fn sync_import_continues_when_relay_already_stored_a_fetched_block() {
        let remote_blocks = build_blocks(3, None);
        let mut local_chain = seeded_chain(&remote_blocks);
        assert!(matches!(
            apply_block(&mut local_chain, &remote_blocks[1], Some("relay")),
            AcceptResult::CanonExtension { height: 2 }
        ));
        let chain = Arc::new(Mutex::new(local_chain));

        let imported =
            import_fetched_blocks(&chain, None, "127.0.0.1:19000", remote_blocks[1..].to_vec())
                .await
                .unwrap();

        assert_eq!(imported, 1);
        let g = chain.lock().await;
        assert_eq!(g.current_height(), 3);
        assert_eq!(g.tip_hash(), remote_blocks[2].hash());
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

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19102".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19103".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19104".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19105".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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
        let (peer_addr, peer_task) =
            spawn_scripted_peer(remote_blocks.clone(), vec![BlockReply::MalformedFrame]).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19106".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19107".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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
        let (peer_addr, peer_task) =
            spawn_scripted_peer(remote_blocks.clone(), vec![BlockReply::Disconnect]).await;

        let local_chain = seeded_chain(&remote_blocks);
        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 6, false);

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19108".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
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
        let valid_blocks = build_blocks(6, None);

        let first_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let first_addr = first_listener.local_addr().unwrap();
        let second_addr = second_listener.local_addr().unwrap();
        let ((malicious_addr, malicious_task), (valid_addr, valid_task)) =
            if first_addr.to_string() < second_addr.to_string() {
                (
                    spawn_scripted_peer_on(first_listener, malicious_blocks.clone(), vec![]),
                    spawn_scripted_peer_on(second_listener, valid_blocks.clone(), vec![]),
                )
            } else {
                (
                    spawn_scripted_peer_on(second_listener, malicious_blocks.clone(), vec![]),
                    spawn_scripted_peer_on(first_listener, valid_blocks.clone(), vec![]),
                )
            };

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

        assert_eq!(pm.peer_summary(&malicious_peer).unwrap().height, 6);
        assert_eq!(pm.peer_summary(&valid_peer).unwrap().height, 6);
        assert_eq!(
            pm.best_sync_target(1).as_deref(),
            Some(malicious_peer.as_str())
        );

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19109".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();

        let first = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
        let first_error = first.expect_err("malicious peer data must be rejected");
        assert!(first_error.to_string().contains("sync import rejected"));
        let g = chain.lock().await;
        assert_eq!(g.current_height(), 1);
        assert_eq!(g.tip_hash(), valid_blocks[0].hash());
        drop(g);

        guard.reset();
        pm.set_state(&malicious_peer, PeerState::Disconnected);
        pm.note_peer_height(&valid_peer, 6, false);
        assert_eq!(pm.best_sync_target(1).as_deref(), Some(valid_peer.as_str()));

        let second = timeout(
            Duration::from_secs(5),
            watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None),
        )
        .await
        .unwrap();
        assert!(second.is_ok());

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 6);
        assert_eq!(g.tip_hash(), valid_blocks.last().unwrap().hash());
        drop(g);

        malicious_task.await.unwrap();
        valid_task.await.unwrap();
    }

    #[tokio::test]
    async fn watchdog_marks_high_risk_when_higher_work_recovery_is_unavailable() {
        let mut chain_state = temp_state();
        let gen = genesis_block();
        assert!(matches!(
            apply_block(&mut chain_state, &gen, None),
            AcceptResult::CanonExtension { height: 0 }
        ));
        let chain = Arc::new(Mutex::new(chain_state));
        let pm = Arc::new(PeerManager::new());
        let peer = "127.0.0.1:9";
        pm.upsert(peer, true);
        pm.set_state(peer, PeerState::Connected);
        pm.note_peer_summary(
            peer,
            ChainSummary::new(1, Some("ff".repeat(32)), 100),
            false,
        );
        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19112".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let recovery = RecoveryState::new();
        let mut guard = SyncGuard::new();

        let result = timeout(
            Duration::from_secs(5),
            watchdog_step(
                &conn_mgr,
                &chain,
                pm.as_ref(),
                &mut guard,
                None,
                Some(&recovery),
            ),
        )
        .await
        .unwrap();

        assert!(result.is_err());
        assert_eq!(
            recovery.mode(),
            crate::node::recovery::RecoveryMode::HighRiskFork
        );
        assert!(recovery.should_pause_mining());
        assert_eq!(recovery.snapshot().peer_addr.as_deref(), Some(peer));
    }
    #[test]
    fn should_sync_for_summary_detects_shorter_higher_work_peer() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("remote-tip".to_string()), 1757),
            false,
        );
        let local = ChainSummary::new(83, Some("local-tip".to_string()), 1754);
        match should_sync_for_summary(&pm, &local) {
            SyncDecision::HigherWork {
                peer_addr,
                remote_work,
                local_work,
            } => {
                assert_eq!(peer_addr, "c:9000");
                assert_eq!(remote_work, 1757);
                assert_eq!(local_work, 1754);
            }
            other => panic!("expected higher-work sync, got {:?}", other),
        }
    }

    #[test]
    fn should_sync_for_summary_ignores_equal_work_branch() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("remote-tip".to_string()), 1754),
            false,
        );
        let local = ChainSummary::new(83, Some("local-tip".to_string()), 1754);
        assert_eq!(should_sync_for_summary(&pm, &local), SyncDecision::Synced);
    }

    #[tokio::test]
    async fn live_sync_requests_shorter_advertised_higher_work_branch() {
        let local_blocks = build_blocks(3, None);
        let remote_blocks = build_alt_blocks(2);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let advertised_work = 99;
        let (peer_addr, peer_task) = spawn_recording_peer_with_work(
            remote_blocks.clone(),
            requests.clone(),
            Some(advertised_work),
        )
        .await;

        let mut local_chain = temp_state();
        let gen = genesis_block();
        assert!(matches!(
            apply_block(&mut local_chain, &gen, None),
            AcceptResult::CanonExtension { height: 0 }
        ));
        for block in &local_blocks {
            assert!(matches!(
                apply_block(&mut local_chain, block, None),
                AcceptResult::CanonExtension { .. }
            ));
        }
        let before_height = local_chain.current_height();
        let before_tip = local_chain.tip_hash();

        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_summary(
            &peer,
            ChainSummary::new(
                2,
                Some(remote_blocks.last().unwrap().hash().to_string()),
                advertised_work,
            ),
            false,
        );

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19111".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let imported = timeout(
            Duration::from_secs(10),
            live_sync_from_peer(&conn_mgr, &chain, pm.as_ref(), &peer, None),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(imported, 2);
        let g = chain.lock().await;
        assert_eq!(g.current_height(), before_height);
        assert_eq!(g.tip_hash(), before_tip);
        assert!(g.side_blocks.contains_key(remote_blocks[0].hash()));
        assert!(g.side_blocks.contains_key(remote_blocks[1].hash()));
        drop(g);

        let requested = requests.lock().await.clone();
        let requested_heights: Vec<u64> = requested
            .iter()
            .map(|hash| {
                remote_blocks
                    .iter()
                    .find(|block| block.hash() == *hash)
                    .map(|block| block.header.number)
                    .unwrap()
            })
            .collect();
        assert_eq!(requested_heights, vec![2, 1]);

        peer_task.await.unwrap();
    }
    #[tokio::test]
    async fn live_sync_from_peer_catches_up_small_gap_after_restart_recovery() {
        let remote_blocks = build_blocks(84, None);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (peer_addr, peer_task) =
            spawn_recording_peer(remote_blocks.clone(), requests.clone()).await;

        let mut local_chain = temp_state();
        let gen = genesis_block();
        assert!(matches!(
            apply_block(&mut local_chain, &gen, None),
            AcceptResult::CanonExtension { height: 0 }
        ));
        for block in remote_blocks.iter().take(81) {
            assert!(matches!(
                apply_block(&mut local_chain, block, None),
                AcceptResult::CanonExtension { .. }
            ));
        }

        let chain = Arc::new(Mutex::new(local_chain));
        let pm = Arc::new(PeerManager::new());
        let peer = peer_addr.to_string();
        pm.upsert(&peer, true);
        pm.set_state(&peer, PeerState::Connected);
        pm.note_peer_height(&peer, 84, false);

        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19110".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let imported = timeout(
            Duration::from_secs(10),
            live_sync_from_peer(&conn_mgr, &chain, pm.as_ref(), &peer, None),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(imported, 3);

        let g = chain.lock().await;
        assert_eq!(g.current_height(), 84);
        assert_eq!(g.tip_hash(), remote_blocks.last().unwrap().hash());
        drop(g);

        let requested = requests.lock().await.clone();
        let requested_heights: Vec<u64> = requested
            .iter()
            .map(|hash| {
                remote_blocks
                    .iter()
                    .find(|block| block.hash() == *hash)
                    .map(|block| block.header.number)
                    .unwrap()
            })
            .collect();
        assert_eq!(requested_heights, vec![84, 83, 82]);

        peer_task.await.unwrap();
    }
}

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::chain::state::ChainState;
use crate::p2p::messages::P2PMessage;
use crate::p2p::peer_manager::{PeerManager, PeerState};
use crate::p2p::protocol::{validate_handshake, ChainSummary, HandshakeMessage, HandshakeResult};

/// Maximum wire message size: 16 MiB.
///
/// Messages larger than this are rejected before reading the body to prevent
/// memory exhaustion attacks from malicious peers.
const MAX_MESSAGE_BYTES: u32 = 16 * 1024 * 1024;

// --- Wire framing -----------------------------------------------------------
//
// Format: [ u32 big-endian length ][ bincode-encoded P2PMessage ]
//
// The length field encodes the number of payload bytes that follow.  A 4-byte
// header keeps the framing simple and avoids delimiter ambiguity.

/// Write a framed `P2PMessage` to any async writer.
///
/// This is generic so it can be called on `TcpStream` in production and on
/// `tokio::io::DuplexStream` (or similar) in tests.
pub async fn send_message<W>(w: &mut W, msg: &P2PMessage) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let payload = bincode::serialize(msg)?;
    let len = payload.len() as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&payload).await?;
    Ok(())
}

/// Read a framed `P2PMessage` from any async reader.
///
/// Returns an error if the length prefix exceeds `MAX_MESSAGE_BYTES`.
pub async fn recv_message<R>(r: &mut R) -> Result<P2PMessage>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_MESSAGE_BYTES {
        anyhow::bail!(
            "oversized message: {} bytes (max {})",
            len,
            MAX_MESSAGE_BYTES
        );
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).await?;
    Ok(bincode::deserialize(&buf)?)
}

// --- Connection manager -----------------------------------------------------

/// Manages inbound TCP P2P connections.
///
/// The manager owns the TCP listener. Each accepted connection is handed off to
/// an independent `tokio::task` via `handle_inbound`. Outbound connections are
/// opened with the static `connect` helper.
pub struct P2PConnectionManager {
    /// Local bind address.
    pub listen_addr: SocketAddr,
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    local_node_nonce: u64,
    advertised_ip: Option<String>,
    advertised_port: Option<u16>,
    allow_private_peer_addresses: bool,
}

impl P2PConnectionManager {
    pub fn new(
        listen_addr: SocketAddr,
        chain: Arc<Mutex<ChainState>>,
        peer_manager: Arc<PeerManager>,
    ) -> Self {
        let local_node_nonce = derive_local_node_nonce(listen_addr);
        Self {
            listen_addr,
            chain,
            peer_manager,
            local_node_nonce,
            advertised_ip: None,
            advertised_port: None,
            allow_private_peer_addresses: true,
        }
    }

    pub fn new_with_advertised(
        listen_addr: SocketAddr,
        chain: Arc<Mutex<ChainState>>,
        peer_manager: Arc<PeerManager>,
        advertised_ip: Option<String>,
        advertised_port: Option<u16>,
        allow_private_peer_addresses: bool,
    ) -> Self {
        let mut manager = Self::new(listen_addr, chain, peer_manager);
        manager.advertised_ip = advertised_ip;
        manager.advertised_port = advertised_port;
        manager.allow_private_peer_addresses = allow_private_peer_addresses;
        manager
    }

    pub(crate) fn local_handshake(&self, chain_height: u64) -> HandshakeMessage {
        HandshakeMessage::new_with_advertised(
            chain_height,
            self.local_node_nonce,
            self.advertised_ip.clone(),
            self.advertised_port,
        )
    }

    /// Nonce used in the local handshake for self-connection detection.
    pub(crate) fn local_node_nonce(&self) -> u64 {
        self.local_node_nonce
    }

    pub async fn bind_listener(&self) -> Result<TcpListener> {
        Ok(TcpListener::bind(self.listen_addr).await?)
    }

    /// Accept inbound connections in a loop, spawning one task per connection.
    pub async fn run_listener(self: Arc<Self>, listener: TcpListener) -> Result<()> {
        tracing::info!("[P2P] listening on {}", self.listen_addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::debug!("[P2P] inbound from {}", addr);
                    let chain = self.chain.clone();
                    let peer_manager = self.peer_manager.clone();
                    let local_node_nonce = self.local_node_nonce;
                    let advertised_ip = self.advertised_ip.clone();
                    let advertised_port = self.advertised_port;
                    let allow_private = self.allow_private_peer_addresses;
                    tokio::spawn(async move {
                        handle_inbound(
                            stream,
                            addr,
                            chain,
                            peer_manager,
                            local_node_nonce,
                            advertised_ip,
                            advertised_port,
                            allow_private,
                        )
                        .await;
                    });
                }
                Err(e) => {
                    tracing::warn!("[P2P] accept error: {}", e);
                }
            }
        }
    }

    /// Open an outbound TCP connection to `peer_addr`.
    pub async fn connect(peer_addr: SocketAddr) -> Result<TcpStream> {
        let stream = TcpStream::connect(peer_addr).await?;
        tracing::debug!("[P2P] connected to {}", peer_addr);
        Ok(stream)
    }
}

const INBOUND_SUMMARY_REFRESH_SOURCE: &str = "inbound handshake refresh";
const INBOUND_SUMMARY_REFRESH_MAX_MESSAGES: usize = 8;

async fn request_inbound_peer_summary<S>(
    stream: &mut S,
    peer_key: &str,
    generation: u64,
    chain: &Arc<Mutex<ChainState>>,
    peer_manager: &Arc<PeerManager>,
) -> Result<()>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    peer_manager.record_height_poll_sent(peer_key);
    send_message(stream, &P2PMessage::GetHeight).await?;

    for _ in 0..INBOUND_SUMMARY_REFRESH_MAX_MESSAGES {
        let msg = tokio::time::timeout(Duration::from_secs(5), recv_message(stream)).await??;
        match msg {
            P2PMessage::Height { summary } => {
                peer_manager.note_peer_summary_from(
                    peer_key,
                    summary.clone(),
                    Some(generation),
                    Some(INBOUND_SUMMARY_REFRESH_SOURCE),
                    false,
                );
                tracing::info!(
                    "[P2P] inbound summary refresh peer={} height={} work={} tip={:?} generation={}",
                    peer_key,
                    summary.height,
                    summary.cumulative_work,
                    summary.tip_hash,
                    generation
                );
                return Ok(());
            }
            P2PMessage::GetHeight => {
                let summary = {
                    let g = chain.lock().await;
                    ChainSummary::from_chain(&g)
                };
                send_message(stream, &P2PMessage::Height { summary }).await?;
            }
            P2PMessage::Ping { timestamp } => {
                send_message(stream, &P2PMessage::Pong { timestamp }).await?;
            }
            P2PMessage::Disconnect { reason } => {
                anyhow::bail!(
                    "peer disconnected during inbound summary refresh: {}",
                    reason
                );
            }
            other => anyhow::bail!(
                "unexpected inbound summary refresh reply: {}",
                other.label()
            ),
        }
    }

    anyhow::bail!(
        "inbound summary refresh exceeded message limit for {}",
        peer_key
    )
}
// --- Inbound message dispatch -----------------------------------------------

fn derive_local_node_nonce(listen_addr: SocketAddr) -> u64 {
    let mut input = Vec::new();
    input.extend_from_slice(listen_addr.to_string().as_bytes());
    input.extend_from_slice(crate::genesis::genesis::GENESIS_HASH.as_bytes());
    input.extend_from_slice(crate::config::constants::NETWORK_ID.as_bytes());
    let hash = blake3::hash(&input);
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_be_bytes(bytes)
}

fn handshake_reject_reason(result: &HandshakeResult) -> String {
    match result {
        HandshakeResult::VersionMismatch { remote, ours } => {
            format!(
                "unsupported protocol version: remote={} ours={}",
                remote, ours
            )
        }
        HandshakeResult::WrongChainId => "wrong chain identity".to_string(),
        HandshakeResult::WrongGenesisHash => "wrong genesis hash".to_string(),
        HandshakeResult::WrongEconHash => "wrong economic version".to_string(),
        HandshakeResult::WrongPowParams => "wrong pow/consensus version".to_string(),
        HandshakeResult::SelfConnection => "self-connection rejected".to_string(),
        HandshakeResult::Accepted => "handshake accepted".to_string(),
    }
}

/// Handle an accepted inbound connection.
///
/// Reads messages in a loop, logging each one by its label. The connection
/// must complete a valid handshake before any other message is accepted.
async fn handle_inbound<S>(
    mut stream: S,
    addr: SocketAddr,
    chain: Arc<Mutex<ChainState>>,
    peer_manager: Arc<PeerManager>,
    local_node_nonce: u64,
    advertised_ip: Option<String>,
    advertised_port: Option<u16>,
    allow_private_peer_addresses: bool,
) where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let mut handshake_done = false;

    loop {
        match recv_message(&mut stream).await {
            Ok(msg) => {
                if !handshake_done && !msg.is_pre_handshake() {
                    tracing::warn!(
                        "[P2P] {} sent {} before handshake - closing",
                        addr,
                        msg.label()
                    );
                    break;
                }

                tracing::trace!("[P2P] <- {} {}", addr, msg.label());

                match msg {
                    P2PMessage::Handshake(remote_hs) => {
                        if handshake_done {
                            tracing::warn!("[P2P] {} sent duplicate handshake - closing", addr);
                            break;
                        }

                        let our_height = chain.lock().await.current_height();
                        let local_hs = HandshakeMessage::new_with_advertised(
                            our_height,
                            local_node_nonce,
                            advertised_ip.clone(),
                            advertised_port,
                        );
                        let validation = validate_handshake(&remote_hs, local_node_nonce);

                        match validation {
                            HandshakeResult::Accepted => {
                                let observed_addr = addr.to_string();
                                let peer_key = match peer_manager.resolve_inbound_peer_key(
                                    &observed_addr,
                                    &remote_hs,
                                    allow_private_peer_addresses,
                                ) {
                                    Ok(peer_key) => peer_key,
                                    Err(reason) => {
                                        let disconnect = P2PMessage::Disconnect { reason };
                                        let _ = send_message(&mut stream, &disconnect).await;
                                        break;
                                    }
                                };
                                peer_manager.set_state(&peer_key, PeerState::Connected);
                                peer_manager.note_peer_height(
                                    &peer_key,
                                    remote_hs.chain_height,
                                    false,
                                );
                                tracing::info!(
                                    "[P2P] {} handshake complete local_height={} remote_height={}",
                                    peer_key,
                                    our_height,
                                    remote_hs.chain_height
                                );
                                if let Err(e) =
                                    send_message(&mut stream, &P2PMessage::Handshake(local_hs))
                                        .await
                                {
                                    tracing::warn!("[P2P] {} handshake send error: {}", addr, e);
                                    break;
                                }
                                tracing::trace!("[P2P] -> {} Handshake accepted", addr);
                                handshake_done = true;
                                if peer_key != observed_addr {
                                    if let Some(generation) =
                                        peer_manager.peer_generation(&peer_key)
                                    {
                                        if let Err(e) = request_inbound_peer_summary(
                                            &mut stream,
                                            &peer_key,
                                            generation,
                                            &chain,
                                            &peer_manager,
                                        )
                                        .await
                                        {
                                            tracing::debug!(
                                                "[P2P] {} inbound summary refresh failed: {}",
                                                peer_key,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            other => {
                                peer_manager.upsert(&addr.to_string(), false);
                                peer_manager.set_state(&addr.to_string(), PeerState::Disconnected);
                                let disconnect = P2PMessage::Disconnect {
                                    reason: handshake_reject_reason(&other),
                                };
                                if let Err(e) = send_message(&mut stream, &disconnect).await {
                                    tracing::warn!("[P2P] {} disconnect send error: {}", addr, e);
                                }
                                tracing::debug!("[P2P] {} handshake rejected: {:?}", addr, other);
                                break;
                            }
                        }
                    }
                    P2PMessage::Ping { timestamp } => {
                        if !handshake_done {
                            tracing::warn!("[P2P] {} ping before handshake - closing", addr);
                            break;
                        }
                        let pong = P2PMessage::Pong { timestamp };
                        if let Err(e) = send_message(&mut stream, &pong).await {
                            tracing::warn!("[P2P] {} pong send error: {}", addr, e);
                            break;
                        }
                        tracing::trace!("[P2P] -> {} Pong", addr);
                    }
                    P2PMessage::GetHeight => {
                        if !handshake_done {
                            tracing::warn!("[P2P] {} getheight before handshake - closing", addr);
                            break;
                        }
                        let summary = {
                            let g = chain.lock().await;
                            ChainSummary::from_chain(&g)
                        };
                        let reply = P2PMessage::Height { summary };
                        if let Err(e) = send_message(&mut stream, &reply).await {
                            tracing::warn!("[P2P] {} height send error: {}", addr, e);
                            break;
                        }
                        tracing::trace!("[P2P] -> {} Height", addr);
                    }
                    P2PMessage::GetBlock { hash } => {
                        if !handshake_done {
                            tracing::warn!("[P2P] {} getblock before handshake - closing", addr);
                            break;
                        }
                        let block = {
                            let g = chain.lock().await;
                            g.block_by_hash(&hash)
                        };
                        match block {
                            Some(block) => {
                                if let Err(e) =
                                    send_message(&mut stream, &P2PMessage::Block { block }).await
                                {
                                    tracing::warn!("[P2P] {} block send error: {}", addr, e);
                                    break;
                                }
                                tracing::trace!("[P2P] -> {} Block {}", addr, hash);
                            }
                            None => {
                                let disconnect = P2PMessage::Disconnect {
                                    reason: format!("unknown block {}", hash),
                                };
                                let _ = send_message(&mut stream, &disconnect).await;
                                tracing::warn!("[P2P] {} requested unknown block {}", addr, hash);
                                break;
                            }
                        }
                    }
                    P2PMessage::Disconnect { reason } => {
                        tracing::debug!("[P2P] {} disconnected: {}", addr, reason);
                        break;
                    }
                    other => {
                        if !handshake_done {
                            tracing::warn!(
                                "[P2P] {} sent {} before handshake - closing",
                                addr,
                                other.label()
                            );
                            break;
                        }
                        tracing::debug!("[P2P] {} {} (unhandled)", addr, other.label());
                    }
                }
            }
            Err(e) => {
                tracing::debug!("[P2P] {} read error: {}", addr, e);
                break;
            }
        }
    }
}

// --- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::genesis_block;
    use crate::p2p::protocol::AnnounceBlock;
    use tokio::io::duplex;
    use tokio::time::{timeout, Duration};

    // Helper: send `msg` from one half of a duplex, recv from the other.
    async fn wire_rt(msg: P2PMessage) -> P2PMessage {
        let (mut tx, mut rx) = duplex(64 * 1024);
        send_message(&mut tx, &msg).await.expect("send");
        recv_message(&mut rx).await.expect("recv")
    }

    fn temp_chain() -> Arc<Mutex<ChainState>> {
        let db = sled::Config::new().temporary(true).open().unwrap();
        Arc::new(Mutex::new(ChainState::empty(db)))
    }

    fn temp_peer_manager() -> Arc<PeerManager> {
        Arc::new(PeerManager::new())
    }

    fn local_nonce_for(addr: SocketAddr) -> u64 {
        derive_local_node_nonce(addr)
    }

    fn accepted_handshake_response(reply: P2PMessage, expected_nonce: u64) -> HandshakeMessage {
        match reply {
            P2PMessage::Handshake(hs) => {
                assert_eq!(hs.node_nonce, expected_nonce);
                hs
            }
            other => panic!("expected handshake response, got {:?}", other),
        }
    }

    // -- framing round-trips -------------------------------------------------

    #[tokio::test]
    async fn ping_frames_correctly() {
        let recv = wire_rt(P2PMessage::Ping { timestamp: 42 }).await;
        assert!(matches!(recv, P2PMessage::Ping { timestamp: 42 }));
    }

    #[tokio::test]
    async fn pong_frames_correctly() {
        let recv = wire_rt(P2PMessage::Pong { timestamp: 999 }).await;
        assert!(matches!(recv, P2PMessage::Pong { timestamp: 999 }));
    }

    #[tokio::test]
    async fn get_height_frames_correctly() {
        let recv = wire_rt(P2PMessage::GetHeight).await;
        assert!(matches!(recv, P2PMessage::GetHeight));
    }

    #[tokio::test]
    async fn height_response_frames_correctly() {
        let msg = P2PMessage::Height {
            summary: ChainSummary::new(77, Some("tip".to_string()), 100),
        };
        match wire_rt(msg).await {
            P2PMessage::Height { summary } => {
                assert_eq!(summary.height, 77);
                assert_eq!(summary.tip_hash.unwrap(), "tip");
                assert_eq!(summary.cumulative_work, 100);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn announce_block_frames_correctly() {
        let ann = AnnounceBlock {
            height: 5,
            hash: "hh".to_string(),
            prev: "pp".to_string(),
        };
        match wire_rt(P2PMessage::AnnounceBlock(ann)).await {
            P2PMessage::AnnounceBlock(a) => {
                assert_eq!(a.height, 5);
                assert_eq!(a.hash, "hh");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn get_block_frames_correctly() {
        let msg = P2PMessage::GetBlock {
            hash: "myhash".to_string(),
        };
        match wire_rt(msg).await {
            P2PMessage::GetBlock { hash } => assert_eq!(hash, "myhash"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn block_frames_correctly() {
        let blk = genesis_block();
        let expected_hash = blk.hash().to_string();
        match wire_rt(P2PMessage::Block { block: blk }).await {
            P2PMessage::Block { block } => assert_eq!(block.hash(), expected_hash),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn handshake_frames_correctly() {
        let hs = HandshakeMessage::new(5, 12345);
        match wire_rt(P2PMessage::Handshake(hs)).await {
            P2PMessage::Handshake(h) => {
                assert_eq!(h.chain_height, 5);
                assert_eq!(h.node_nonce, 12345);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn disconnect_frames_correctly() {
        let msg = P2PMessage::Disconnect {
            reason: "test".to_string(),
        };
        match wire_rt(msg).await {
            P2PMessage::Disconnect { reason } => assert_eq!(reason, "test"),
            _ => panic!("wrong variant"),
        }
    }

    // -- framing safety ------------------------------------------------------

    #[tokio::test]
    async fn oversized_message_is_rejected() {
        let (mut tx, mut rx) = duplex(64 * 1024);
        let huge_len: u32 = MAX_MESSAGE_BYTES + 1;
        tx.write_all(&huge_len.to_be_bytes()).await.unwrap();
        let err = recv_message(&mut rx).await;
        assert!(err.is_err(), "oversized message should produce an error");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("oversized"),
            "error should mention oversized: {}",
            msg
        );
    }

    #[tokio::test]
    async fn multiple_messages_frame_independently() {
        let (mut tx, mut rx) = duplex(64 * 1024);
        send_message(&mut tx, &P2PMessage::Ping { timestamp: 1 })
            .await
            .unwrap();
        send_message(&mut tx, &P2PMessage::GetHeight).await.unwrap();
        send_message(&mut tx, &P2PMessage::Pong { timestamp: 2 })
            .await
            .unwrap();

        assert!(matches!(
            recv_message(&mut rx).await.unwrap(),
            P2PMessage::Ping { timestamp: 1 }
        ));
        assert!(matches!(
            recv_message(&mut rx).await.unwrap(),
            P2PMessage::GetHeight
        ));
        assert!(matches!(
            recv_message(&mut rx).await.unwrap(),
            P2PMessage::Pong { timestamp: 2 }
        ));
    }

    // -- handshake validation wiring ----------------------------------------

    #[tokio::test]
    async fn inbound_handshake_accepts_and_allows_followup_messages() {
        let addr: SocketAddr = "127.0.0.1:19001".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain.clone(),
            peer_manager.clone(),
            local_nonce,
            None,
            None,
            true,
        ));

        let remote = HandshakeMessage::new(17, local_nonce + 1);
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        let reply = recv_message(&mut client).await.unwrap();
        let local_hs = accepted_handshake_response(reply, local_nonce);
        assert_eq!(
            local_hs.protocol_version,
            crate::config::constants::PROTOCOL_VERSION
        );

        send_message(&mut client, &P2PMessage::Ping { timestamp: 42 })
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Pong { timestamp } => assert_eq!(timestamp, 42),
            other => panic!("expected pong, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(peer_manager.connected_count(), 1);
        assert_eq!(peer_manager.best_remote_height(), 17);
    }

    #[tokio::test]
    async fn inbound_announcements_and_blocks_do_not_change_chain_without_sync() {
        let addr: SocketAddr = "127.0.0.1:19008".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain.clone(),
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let remote = HandshakeMessage::new(0, local_nonce + 1);
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        let reply = recv_message(&mut client).await.unwrap();
        accepted_handshake_response(reply, local_nonce);

        let block = genesis_block();
        let block_hash = block.hash().to_string();
        let announcement = AnnounceBlock {
            height: block.header.number,
            hash: block_hash.clone(),
            prev: block.header.parent_hash.clone(),
        };
        send_message(&mut client, &P2PMessage::AnnounceBlock(announcement))
            .await
            .unwrap();
        send_message(&mut client, &P2PMessage::Block { block })
            .await
            .unwrap();

        // Ping/Pong provides a deterministic barrier proving the inbound loop
        // consumed both preceding messages before the chain is inspected.
        send_message(&mut client, &P2PMessage::Ping { timestamp: 43 })
            .await
            .unwrap();
        assert!(matches!(
            recv_message(&mut client).await.unwrap(),
            P2PMessage::Pong { timestamp: 43 }
        ));

        assert!(chain.lock().await.block_by_hash(&block_hash).is_none());

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn inbound_advertised_peer_refreshes_full_summary_on_active_stream() {
        let addr: SocketAddr = "127.0.0.1:19009".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain.clone(),
            peer_manager.clone(),
            local_nonce,
            Some("127.0.0.1".to_string()),
            Some(19010),
            true,
        ));

        let remote = HandshakeMessage::new_with_advertised(
            134,
            local_nonce + 1,
            Some("127.0.0.1".to_string()),
            Some(61129),
        );
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        let reply = recv_message(&mut client).await.unwrap();
        let _local_hs = accepted_handshake_response(reply, local_nonce);

        send_message(&mut client, &P2PMessage::GetHeight)
            .await
            .unwrap();

        let mut saw_server_height = false;
        let mut answered_server_get_height = false;
        for _ in 0..2 {
            match recv_message(&mut client).await.unwrap() {
                P2PMessage::GetHeight => {
                    answered_server_get_height = true;
                    send_message(
                        &mut client,
                        &P2PMessage::Height {
                            summary: ChainSummary::new(
                                134,
                                Some("02ab8b991ecfbdecf5caaa532b58fa08a0ff20361ede688153edb825a9950977".to_string()),
                                7608,
                            ),
                        },
                    )
                    .await
                    .unwrap();
                }
                P2PMessage::Height { summary } => {
                    saw_server_height = true;
                    assert_eq!(summary.height, 0);
                }
                other => panic!("unexpected message during summary refresh: {:?}", other),
            }
        }
        assert!(answered_server_get_height);
        assert!(saw_server_height);

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();

        let key = "127.0.0.1:61129";
        let summary = peer_manager.peer_summary(key).unwrap();
        assert_eq!(summary.height, 134);
        assert_eq!(summary.cumulative_work, 7608);
        assert_eq!(
            summary.tip_hash.as_deref(),
            Some("02ab8b991ecfbdecf5caaa532b58fa08a0ff20361ede688153edb825a9950977")
        );
        assert_eq!(
            peer_manager.best_work_sync_target("local", 7517),
            Some(key.to_string())
        );

        let peer = peer_manager
            .snapshot()
            .into_iter()
            .find(|peer| peer.addr == key)
            .unwrap();
        assert_eq!(peer.observed_addr.as_deref(), Some("127.0.0.1:19009"));
        assert_eq!(peer.advertised_addr.as_deref(), Some(key));
        assert_eq!(peer.connection_generation, 1);
        assert_eq!(peer.summary_generation, Some(1));
        assert_eq!(
            peer.summary_source.as_deref(),
            Some(INBOUND_SUMMARY_REFRESH_SOURCE)
        );
    }
    #[tokio::test]
    async fn inbound_handshake_rejects_wrong_chain_id() {
        let addr: SocketAddr = "127.0.0.1:19002".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let mut remote = HandshakeMessage::new(0, local_nonce + 1);
        remote.chain_id = [0xAA; 32];
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Disconnect { reason } => {
                assert_eq!(reason, "wrong chain identity");
            }
            other => panic!("expected disconnect, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn inbound_handshake_rejects_wrong_genesis_hash() {
        let addr: SocketAddr = "127.0.0.1:19003".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let mut remote = HandshakeMessage::new(0, local_nonce + 1);
        remote.genesis_hash = "00".repeat(32);
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Disconnect { reason } => {
                assert_eq!(reason, "wrong genesis hash");
            }
            other => panic!("expected disconnect, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn inbound_handshake_rejects_wrong_economic_version() {
        let addr: SocketAddr = "127.0.0.1:19004".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let mut remote = HandshakeMessage::new(0, local_nonce + 1);
        remote.econ_hash = "bad".repeat(16);
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Disconnect { reason } => {
                assert_eq!(reason, "wrong economic version");
            }
            other => panic!("expected disconnect, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn inbound_handshake_rejects_wrong_pow_version() {
        let addr: SocketAddr = "127.0.0.1:19005".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let mut remote = HandshakeMessage::new(0, local_nonce + 1);
        remote.pow_params_hash = "bad".repeat(16);
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Disconnect { reason } => {
                assert_eq!(reason, "wrong pow/consensus version");
            }
            other => panic!("expected disconnect, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn inbound_handshake_rejects_unsupported_protocol_version() {
        let addr: SocketAddr = "127.0.0.1:19006".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let mut remote = HandshakeMessage::new(0, local_nonce + 1);
        remote.protocol_version = crate::config::constants::PROTOCOL_VERSION + 1;
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Disconnect { reason } => {
                assert_eq!(
                    reason,
                    format!(
                        "unsupported protocol version: remote={} ours={}",
                        crate::config::constants::PROTOCOL_VERSION + 1,
                        crate::config::constants::PROTOCOL_VERSION
                    )
                );
            }
            other => panic!("expected disconnect, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn inbound_handshake_rejects_self_connection() {
        let addr: SocketAddr = "127.0.0.1:19007".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        let remote = HandshakeMessage::new(0, local_nonce);
        send_message(&mut client, &P2PMessage::Handshake(remote))
            .await
            .unwrap();
        match recv_message(&mut client).await.unwrap() {
            P2PMessage::Disconnect { reason } => {
                assert_eq!(reason, "self-connection rejected");
            }
            other => panic!("expected disconnect, got {:?}", other),
        }

        drop(client);
        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn malformed_handshake_does_not_panic() {
        let addr: SocketAddr = "127.0.0.1:19008".parse().unwrap();
        let local_nonce = local_nonce_for(addr);
        let chain = temp_chain();
        let peer_manager = temp_peer_manager();
        let (server, mut client) = duplex(64 * 1024);
        let handle = tokio::spawn(handle_inbound(
            server,
            addr,
            chain,
            peer_manager,
            local_nonce,
            None,
            None,
            true,
        ));

        client.write_all(&4u32.to_be_bytes()).await.unwrap();
        client.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).await.unwrap();
        drop(client);

        timeout(Duration::from_secs(2), handle)
            .await
            .unwrap()
            .unwrap();
    }
}

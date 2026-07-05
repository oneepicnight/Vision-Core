use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;
use crate::p2p::messages::P2PMessage;

/// Maximum wire message size: 16 MiB.
///
/// Messages larger than this are rejected before reading the body to prevent
/// memory exhaustion attacks from malicious peers.
const MAX_MESSAGE_BYTES: u32 = 16 * 1024 * 1024;

// ─── Wire framing ─────────────────────────────────────────────────────────────
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
        anyhow::bail!("oversized message: {} bytes (max {})", len, MAX_MESSAGE_BYTES);
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).await?;
    Ok(bincode::deserialize(&buf)?)
}

// ─── Connection manager ───────────────────────────────────────────────────────

/// Manages inbound TCP P2P connections.
///
/// The manager owns the TCP listener. Each accepted connection is handed off to
/// an independent `tokio::task` via `handle_inbound`. Outbound connections are
/// opened with the static `connect` helper.
pub struct P2PConnectionManager {
    /// Local bind address.
    pub listen_addr: SocketAddr,
}

impl P2PConnectionManager {
    pub fn new(listen_addr: SocketAddr) -> Self {
        Self { listen_addr }
    }

    /// Accept inbound connections in a loop, spawning one task per connection.
    pub async fn run_listener(self: Arc<Self>) -> Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        tracing::info!("[P2P] listening on {}", self.listen_addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::debug!("[P2P] inbound from {}", addr);
                    tokio::spawn(handle_inbound(stream, addr));
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

// ─── Inbound message dispatch ─────────────────────────────────────────────────

/// Handle an accepted inbound connection.
///
/// Reads messages in a loop, logging each one by its label. All processing
/// past the handshake is deferred to future work items (sync, mempool relay,
/// etc.).  The connection is closed on any I/O or decode error.
async fn handle_inbound(mut stream: TcpStream, addr: SocketAddr) {
    let mut handshake_done = false;

    loop {
        match recv_message(&mut stream).await {
            Ok(msg) => {
                // Before handshake: only Handshake or Disconnect are legal.
                if !handshake_done && !msg.is_pre_handshake() {
                    tracing::warn!("[P2P] {} sent {} before handshake — closing", addr, msg.label());
                    break;
                }

                tracing::trace!("[P2P] ← {} {}", addr, msg.label());

                match msg {
                    P2PMessage::Handshake(_h) => {
                        // TODO: call validate_handshake, send our own Handshake back.
                        handshake_done = true;
                    }
                    P2PMessage::Ping { timestamp } => {
                        let pong = P2PMessage::Pong { timestamp };
                        if let Err(e) = send_message(&mut stream, &pong).await {
                            tracing::warn!("[P2P] {} pong send error: {}", addr, e);
                            break;
                        }
                        tracing::trace!("[P2P] → {} Pong", addr);
                    }
                    P2PMessage::Disconnect { reason } => {
                        tracing::debug!("[P2P] {} disconnected: {}", addr, reason);
                        break;
                    }
                    // All other messages are logged and ignored until the sync
                    // layer is wired in (Prompt 10).
                    other => {
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::protocol::{AnnounceBlock, HandshakeMessage};
    use crate::genesis::genesis_block;
    use tokio::io::duplex;

    // Helper: send `msg` from one half of a duplex, recv from the other.
    async fn wire_rt(msg: P2PMessage) -> P2PMessage {
        let (mut tx, mut rx) = duplex(64 * 1024);
        send_message(&mut tx, &msg).await.expect("send");
        recv_message(&mut rx).await.expect("recv")
    }

    // ── framing round-trips ───────────────────────────────────────────────────

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
        let msg = P2PMessage::Height { height: 77, tip_hash: Some("tip".to_string()) };
        match wire_rt(msg).await {
            P2PMessage::Height { height, tip_hash } => {
                assert_eq!(height, 77);
                assert_eq!(tip_hash.unwrap(), "tip");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn announce_block_frames_correctly() {
        let ann = AnnounceBlock { height: 5, hash: "hh".to_string(), prev: "pp".to_string() };
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
        let msg = P2PMessage::GetBlock { hash: "myhash".to_string() };
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
        let msg = P2PMessage::Disconnect { reason: "test".to_string() };
        match wire_rt(msg).await {
            P2PMessage::Disconnect { reason } => assert_eq!(reason, "test"),
            _ => panic!("wrong variant"),
        }
    }

    // ── framing safety ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn oversized_message_is_rejected() {
        let (mut tx, mut rx) = duplex(64 * 1024);
        // Write a length prefix that exceeds MAX_MESSAGE_BYTES.
        let huge_len: u32 = MAX_MESSAGE_BYTES + 1;
        tx.write_all(&huge_len.to_be_bytes()).await.unwrap();
        let err = recv_message(&mut rx).await;
        assert!(err.is_err(), "oversized message should produce an error");
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("oversized"), "error should mention oversized: {}", msg);
    }

    #[tokio::test]
    async fn multiple_messages_frame_independently() {
        let (mut tx, mut rx) = duplex(64 * 1024);
        send_message(&mut tx, &P2PMessage::Ping { timestamp: 1 }).await.unwrap();
        send_message(&mut tx, &P2PMessage::GetHeight).await.unwrap();
        send_message(&mut tx, &P2PMessage::Pong { timestamp: 2 }).await.unwrap();

        assert!(matches!(recv_message(&mut rx).await.unwrap(), P2PMessage::Ping { timestamp: 1 }));
        assert!(matches!(recv_message(&mut rx).await.unwrap(), P2PMessage::GetHeight));
        assert!(matches!(recv_message(&mut rx).await.unwrap(), P2PMessage::Pong { timestamp: 2 }));
    }
}

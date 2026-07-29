use crate::p2p::protocol::{AnnounceBlock, ChainSummary, HandshakeMessage};
use crate::types::Block;
use serde::{Deserialize, Serialize};

/// All message types exchanged between peers over the TCP P2P connection.
///
/// This is the **core** message set — only what is needed for liveness,
/// height exchange, and block propagation. Adding a new variant is a protocol
/// change that requires bumping `PROTOCOL_VERSION`.
///
/// Encoding: bincode, length-prefixed (see `connection::send_message`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    // ── Handshake ─────────────────────────────────────────────────────────────
    /// First message sent on **both** sides of every new connection.
    ///
    /// Peers MUST close the connection if any field fails validation:
    /// `protocol_version`, `genesis_hash`, `chain_id`, or `econ_hash`.
    Handshake(HandshakeMessage),

    // ── Liveness ─────────────────────────────────────────────────────────────
    /// Keep-alive probe; carries the sender's UNIX timestamp (seconds).
    Ping { timestamp: u64 },

    /// Keep-alive reply — MUST echo the exact `timestamp` from the Ping.
    Pong { timestamp: u64 },

    // ── Height exchange ───────────────────────────────────────────────────────
    /// Ask a peer for its current canonical chain summary.
    GetHeight,

    /// Response to `GetHeight`.
    Height { summary: ChainSummary },

    // ── Block propagation ─────────────────────────────────────────────────────
    /// Lightweight block announcement. Receivers request the full block only
    /// if they have not already seen `hash`.
    AnnounceBlock(AnnounceBlock),

    /// Request a single block by its PoW hash.
    GetBlock { hash: String },

    /// Full block body — response to `GetBlock` or unsolicited push after mining.
    Block { block: Block },

    // ── Connection management ─────────────────────────────────────────────────
    /// Graceful disconnect with a human-readable reason string.
    Disconnect { reason: String },
}

impl P2PMessage {
    /// Short static label used in tracing / logging.
    ///
    /// Every variant maps to a distinct string so log lines are greppable
    /// without having to print the full message payload.
    pub fn label(&self) -> &'static str {
        match self {
            P2PMessage::Handshake(_) => "Handshake",
            P2PMessage::Ping { .. } => "Ping",
            P2PMessage::Pong { .. } => "Pong",
            P2PMessage::GetHeight => "GetHeight",
            P2PMessage::Height { .. } => "Height",
            P2PMessage::AnnounceBlock(_) => "AnnounceBlock",
            P2PMessage::GetBlock { .. } => "GetBlock",
            P2PMessage::Block { .. } => "Block",
            P2PMessage::Disconnect { .. } => "Disconnect",
        }
    }

    /// `true` for messages that are acceptable before the handshake completes.
    ///
    /// Only `Handshake` and `Disconnect` are valid pre-handshake; all other
    /// messages from an unverified peer should be dropped.
    pub fn is_pre_handshake(&self) -> bool {
        matches!(
            self,
            P2PMessage::Handshake(_) | P2PMessage::Disconnect { .. }
        )
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::genesis_block;
    use crate::p2p::protocol::HandshakeMessage;

    // Helper: bincode round-trip.
    fn rt(msg: &P2PMessage) -> P2PMessage {
        let bytes = bincode::serialize(msg).expect("serialize");
        bincode::deserialize(&bytes).expect("deserialize")
    }

    // ── encode / decode ───────────────────────────────────────────────────────

    #[test]
    fn ping_round_trips() {
        let rt = rt(&P2PMessage::Ping {
            timestamp: 1_700_000_000,
        });
        assert!(matches!(
            rt,
            P2PMessage::Ping {
                timestamp: 1_700_000_000
            }
        ));
    }

    #[test]
    fn pong_round_trips() {
        let rt = rt(&P2PMessage::Pong { timestamp: 99_999 });
        assert!(matches!(rt, P2PMessage::Pong { timestamp: 99_999 }));
    }

    #[test]
    fn get_height_round_trips() {
        assert!(matches!(rt(&P2PMessage::GetHeight), P2PMessage::GetHeight));
    }

    #[test]
    fn height_with_hash_round_trips() {
        let msg = P2PMessage::Height {
            summary: ChainSummary::new(42, Some("abc".to_string()), 100),
        };
        match rt(&msg) {
            P2PMessage::Height { summary } => {
                assert_eq!(summary.height, 42);
                assert_eq!(summary.tip_hash.unwrap(), "abc");
                assert_eq!(summary.cumulative_work, 100);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn height_with_none_hash_round_trips() {
        let msg = P2PMessage::Height {
            summary: ChainSummary::new(0, None, 0),
        };
        match rt(&msg) {
            P2PMessage::Height { summary } => {
                assert_eq!(summary.height, 0);
                assert!(summary.tip_hash.is_none());
                assert_eq!(summary.cumulative_work, 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn announce_block_round_trips() {
        let ann = AnnounceBlock {
            height: 7,
            hash: "aabbcc".to_string(),
            prev: "001122".to_string(),
        };
        match rt(&P2PMessage::AnnounceBlock(ann)) {
            P2PMessage::AnnounceBlock(a) => {
                assert_eq!(a.height, 7);
                assert_eq!(a.hash, "aabbcc");
                assert_eq!(a.prev, "001122");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn get_block_round_trips() {
        let msg = P2PMessage::GetBlock {
            hash: "hashXYZ".to_string(),
        };
        match rt(&msg) {
            P2PMessage::GetBlock { hash } => assert_eq!(hash, "hashXYZ"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn block_round_trips() {
        let blk = genesis_block();
        let hash = blk.hash().to_string();
        match rt(&P2PMessage::Block { block: blk }) {
            P2PMessage::Block { block } => assert_eq!(block.hash(), hash),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn disconnect_round_trips() {
        let msg = P2PMessage::Disconnect {
            reason: "test bye".to_string(),
        };
        match rt(&msg) {
            P2PMessage::Disconnect { reason } => assert_eq!(reason, "test bye"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn handshake_round_trips() {
        let hs = HandshakeMessage::new(10, 12345);
        match rt(&P2PMessage::Handshake(hs)) {
            P2PMessage::Handshake(h) => {
                assert_eq!(h.chain_height, 10);
                assert_eq!(h.node_nonce, 12345);
            }
            _ => panic!("wrong variant"),
        }
    }

    // ── label ─────────────────────────────────────────────────────────────────

    #[test]
    fn all_variants_have_distinct_labels() {
        use std::collections::HashSet;
        let msgs: Vec<P2PMessage> = vec![
            P2PMessage::Handshake(HandshakeMessage::new(0, 0)),
            P2PMessage::Ping { timestamp: 0 },
            P2PMessage::Pong { timestamp: 0 },
            P2PMessage::GetHeight,
            P2PMessage::Height {
                summary: ChainSummary::new(0, None, 0),
            },
            P2PMessage::AnnounceBlock(AnnounceBlock {
                height: 0,
                hash: String::new(),
                prev: String::new(),
            }),
            P2PMessage::GetBlock {
                hash: String::new(),
            },
            P2PMessage::Block {
                block: genesis_block(),
            },
            P2PMessage::Disconnect {
                reason: String::new(),
            },
        ];
        let labels: HashSet<_> = msgs.iter().map(|m| m.label()).collect();
        assert_eq!(labels.len(), msgs.len(), "duplicate labels");
    }

    #[test]
    fn label_matches_variant_name() {
        assert_eq!(P2PMessage::GetHeight.label(), "GetHeight");
        assert_eq!(P2PMessage::Ping { timestamp: 0 }.label(), "Ping");
        assert_eq!(P2PMessage::Pong { timestamp: 0 }.label(), "Pong");
    }

    // ── is_pre_handshake ──────────────────────────────────────────────────────

    #[test]
    fn only_handshake_and_disconnect_allowed_pre_handshake() {
        assert!(P2PMessage::Handshake(HandshakeMessage::new(0, 0)).is_pre_handshake());
        assert!(P2PMessage::Disconnect {
            reason: String::new()
        }
        .is_pre_handshake());
        assert!(!P2PMessage::Ping { timestamp: 0 }.is_pre_handshake());
        assert!(!P2PMessage::GetHeight.is_pre_handshake());
        assert!(!P2PMessage::GetBlock {
            hash: String::new()
        }
        .is_pre_handshake());
    }
}

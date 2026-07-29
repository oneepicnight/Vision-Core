use crate::config::constants::{CONSENSUS_VERSION, NETWORK_ID, NODE_VERSION, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};

// ─── Handshake ────────────────────────────────────────────────────────────────
/// Canonical chain summary advertised during P2P height polling.
///
/// `cumulative_work` is a discovery hint only. A node must still fetch and
/// validate every block through normal acceptance before changing canonical
/// state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainSummary {
    pub height: u64,
    pub tip_hash: Option<String>,
    pub cumulative_work: u128,
}

impl ChainSummary {
    pub fn new(height: u64, tip_hash: Option<String>, cumulative_work: u128) -> Self {
        Self {
            height,
            tip_hash,
            cumulative_work,
        }
    }

    pub fn from_chain(g: &crate::chain::state::ChainState) -> Self {
        let tip_hash = if g.blocks.is_empty() {
            None
        } else {
            Some(g.tip_hash())
        };
        let cumulative_work = tip_hash
            .as_deref()
            .and_then(|hash| g.cumulative_work.get(hash).copied())
            .unwrap_or(0);
        Self {
            height: g.current_height(),
            tip_hash,
            cumulative_work,
        }
    }
}

/// Exchanged immediately after TCP connection establishment (both directions).
///
/// Peers whose `protocol_version`, `genesis_hash`, `chain_id`, or `econ_hash`
/// do not match MUST be disconnected immediately — they are on a different
/// network, a different fork, or running incompatible economics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeMessage {
    /// Must equal `PROTOCOL_VERSION`; reject the peer if it does not.
    pub protocol_version: u32,

    /// 32-byte network identifier: blake3(`genesis_hash` + `network_id`).
    /// Provides a single-field mismatch check for network/fork partitioning.
    pub chain_id: [u8; 32],

    /// Canonical genesis block hash. Must match `GENESIS_HASH`.
    pub genesis_hash: String,

    /// Random nonce chosen fresh per TCP connection. Used to detect and reject
    /// self-connections (same node, different ports).
    pub node_nonce: u64,

    /// Sender's current canonical chain height.
    pub chain_height: u64,

    /// Network name string (e.g. `"mainnet"`).
    pub network_id: String,

    /// Human-readable version tag for diagnostics only (e.g. `"v1.0.4-consensus-v1.0.3"`).
    pub node_tag: String,

    /// Economics fingerprint — must match `ECON_HASH` on all nodes to prevent
    /// silently mis-matched reward schedules from forming a parallel network.
    pub econ_hash: String,

    /// VisionX PoW params fingerprint — must match on all nodes.
    pub pow_params_hash: String,

    /// Advertised external IP for inbound peer routing (optional).
    pub advertised_ip: Option<String>,

    /// Advertised external P2P port (optional).
    pub advertised_port: Option<u16>,

    /// Seed peers this node knows about, shared at handshake for peer exchange.
    pub seed_peers: Vec<String>,
}

impl HandshakeMessage {
    /// Build a handshake for the local node at the given chain height.
    ///
    /// `node_nonce` should be a fresh random value per connection.
    pub fn new(chain_height: u64, node_nonce: u64) -> Self {
        use crate::genesis::genesis::{ECON_HASH, GENESIS_HASH};
        use crate::pow::visionx::VISIONX_PARAMS;

        let chain_id = {
            let mut input = Vec::new();
            input.extend_from_slice(GENESIS_HASH.as_bytes());
            input.extend_from_slice(NETWORK_ID.as_bytes());
            let hash = blake3::hash(&input);
            let mut id = [0u8; 32];
            id.copy_from_slice(hash.as_bytes());
            id
        };

        Self {
            protocol_version: PROTOCOL_VERSION,
            chain_id,
            genesis_hash: GENESIS_HASH.to_string(),
            node_nonce,
            chain_height,
            network_id: NETWORK_ID.to_string(),
            node_tag: format!("{}-consensus-v1.0.{}", NODE_VERSION, CONSENSUS_VERSION),
            econ_hash: ECON_HASH.to_string(),
            pow_params_hash: VISIONX_PARAMS.fingerprint(),
            advertised_ip: None,
            advertised_port: None,
            seed_peers: vec![],
        }
    }
    pub fn new_with_advertised(
        chain_height: u64,
        node_nonce: u64,
        advertised_ip: Option<String>,
        advertised_port: Option<u16>,
    ) -> Self {
        let mut handshake = Self::new(chain_height, node_nonce);
        handshake.advertised_ip = advertised_ip;
        handshake.advertised_port = advertised_port;
        handshake
    }
}

// ─── Handshake validation ─────────────────────────────────────────────────────

/// Outcome of validating a remote peer's handshake message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeResult {
    /// All fields match; the peer is compatible.
    Accepted,
    /// `protocol_version` does not match ours.
    VersionMismatch { remote: u32, ours: u32 },
    /// `chain_id` bytes differ (wrong network or fork).
    WrongChainId,
    /// `genesis_hash` string differs from `GENESIS_HASH`.
    WrongGenesisHash,
    /// `econ_hash` differs — remote runs different economics.
    WrongEconHash,
    /// `pow_params_hash` differs — remote uses different PoW params.
    WrongPowParams,
    /// Remote's `node_nonce` matches our own — self-connection detected.
    SelfConnection,
}

/// Validate a handshake received from a remote peer.
///
/// `our_nonce` is the nonce we sent in *our* handshake on this connection.
pub fn validate_handshake(remote: &HandshakeMessage, our_nonce: u64) -> HandshakeResult {
    use crate::genesis::genesis::{ECON_HASH, GENESIS_HASH};
    use crate::pow::visionx::VISIONX_PARAMS;

    if remote.node_nonce == our_nonce {
        return HandshakeResult::SelfConnection;
    }
    if remote.protocol_version != PROTOCOL_VERSION {
        return HandshakeResult::VersionMismatch {
            remote: remote.protocol_version,
            ours: PROTOCOL_VERSION,
        };
    }
    // chain_id encodes genesis + network in one field — check it first.
    let expected_chain_id = {
        let mut input = Vec::new();
        input.extend_from_slice(GENESIS_HASH.as_bytes());
        input.extend_from_slice(NETWORK_ID.as_bytes());
        let hash = blake3::hash(&input);
        let mut id = [0u8; 32];
        id.copy_from_slice(hash.as_bytes());
        id
    };
    if remote.chain_id != expected_chain_id {
        return HandshakeResult::WrongChainId;
    }
    if remote.genesis_hash != GENESIS_HASH {
        return HandshakeResult::WrongGenesisHash;
    }
    if remote.econ_hash != ECON_HASH {
        return HandshakeResult::WrongEconHash;
    }
    if remote.pow_params_hash != VISIONX_PARAMS.fingerprint() {
        return HandshakeResult::WrongPowParams;
    }
    HandshakeResult::Accepted
}

// ─── Block gossip protocol ────────────────────────────────────────────────────

/// Lightweight block announcement broadcast to all connected peers.
///
/// Upon receiving this, a peer checks whether it already has `hash`. If not,
/// it sends `GetBlock { hash }` to the announcing peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceBlock {
    /// Height of the announced block.
    pub height: u64,
    /// PoW hash of the announced block (hex-encoded, 64 chars).
    pub hash: String,
    /// Parent hash — allows orphan detection without fetching the block first.
    pub prev: String,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::constants::PROTOCOL_VERSION;

    /// Build the canonical "good" handshake for our node.
    fn our_hs(height: u64, nonce: u64) -> HandshakeMessage {
        HandshakeMessage::new(height, nonce)
    }

    // ── HandshakeMessage::new ─────────────────────────────────────────────────

    #[test]
    fn new_sets_correct_protocol_version() {
        let hs = our_hs(0, 1);
        assert_eq!(hs.protocol_version, PROTOCOL_VERSION);
    }


    #[test]
    fn new_with_advertised_sets_peer_identity_fields() {
        let hs = HandshakeMessage::new_with_advertised(
            10,
            99,
            Some("127.0.0.1".to_string()),
            Some(7072),
        );
        assert_eq!(hs.advertised_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(hs.advertised_port, Some(7072));
    }

    #[test]
    fn new_sets_consensus_version_in_node_tag() {
        let hs = our_hs(0, 1);
        assert!(hs.node_tag.contains("consensus-v1.0.3"));
    }
    #[test]
    fn new_sets_correct_network_id() {
        let hs = our_hs(0, 1);
        assert_eq!(hs.network_id, NETWORK_ID);
    }

    #[test]
    fn new_chain_id_is_deterministic() {
        let a = our_hs(0, 1);
        let b = our_hs(5, 99);
        assert_eq!(
            a.chain_id, b.chain_id,
            "chain_id must not depend on height/nonce"
        );
    }

    #[test]
    fn new_stores_height_and_nonce() {
        let hs = our_hs(42, 7777);
        assert_eq!(hs.chain_height, 42);
        assert_eq!(hs.node_nonce, 7777);
    }

    // ── validate_handshake — happy path ───────────────────────────────────────

    #[test]
    fn valid_peer_is_accepted() {
        let our_nonce = 100;
        let their_nonce = 999;
        let peer = our_hs(10, their_nonce);
        assert_eq!(
            validate_handshake(&peer, our_nonce),
            HandshakeResult::Accepted
        );
    }

    #[test]
    fn different_heights_still_accepted() {
        // Height is informational; it does NOT cause rejection.
        let peer = our_hs(9999, 42);
        assert_eq!(validate_handshake(&peer, 1), HandshakeResult::Accepted);
    }

    // ── validate_handshake — rejection cases ─────────────────────────────────

    #[test]
    fn self_connection_detected() {
        let nonce = 55;
        let hs = our_hs(0, nonce);
        assert_eq!(
            validate_handshake(&hs, nonce),
            HandshakeResult::SelfConnection
        );
    }

    #[test]
    fn version_mismatch_rejected() {
        let mut peer = our_hs(0, 2);
        peer.protocol_version = PROTOCOL_VERSION + 1;
        assert_eq!(
            validate_handshake(&peer, 1),
            HandshakeResult::VersionMismatch {
                remote: PROTOCOL_VERSION + 1,
                ours: PROTOCOL_VERSION,
            }
        );
    }

    #[test]
    fn v1_0_2_protocol_peer_is_rejected() {
        let mut peer = our_hs(0, 2);
        peer.protocol_version = 3;
        assert_eq!(
            validate_handshake(&peer, 1),
            HandshakeResult::VersionMismatch {
                remote: 3,
                ours: PROTOCOL_VERSION,
            }
        );
    }
    #[test]
    fn wrong_chain_id_rejected() {
        let mut peer = our_hs(0, 2);
        peer.chain_id = [0xFFu8; 32];
        assert_eq!(validate_handshake(&peer, 1), HandshakeResult::WrongChainId);
    }

    #[test]
    fn wrong_genesis_hash_rejected() {
        let mut peer = our_hs(0, 2);
        peer.genesis_hash = "0".repeat(64);
        // chain_id will also differ, so chain_id check fires first.
        // Force the chain_id to pass so we can isolate genesis_hash check.
        use crate::genesis::genesis::GENESIS_HASH;
        let mut input = Vec::new();
        input.extend_from_slice(GENESIS_HASH.as_bytes());
        input.extend_from_slice(NETWORK_ID.as_bytes());
        let hash = blake3::hash(&input);
        hash.as_bytes()
            .iter()
            .enumerate()
            .for_each(|(i, &b)| peer.chain_id[i] = b);
        // Now flip just genesis_hash.
        peer.genesis_hash = "0".repeat(64);
        assert_eq!(
            validate_handshake(&peer, 1),
            HandshakeResult::WrongGenesisHash
        );
    }

    #[test]
    fn wrong_econ_hash_rejected() {
        let mut peer = our_hs(0, 2);
        peer.econ_hash = "badhash".to_string();
        assert_eq!(validate_handshake(&peer, 1), HandshakeResult::WrongEconHash);
    }

    #[test]
    fn wrong_pow_params_rejected() {
        let mut peer = our_hs(0, 2);
        peer.pow_params_hash = "wrongparams".to_string();
        assert_eq!(
            validate_handshake(&peer, 1),
            HandshakeResult::WrongPowParams
        );
    }

    // ── AnnounceBlock ─────────────────────────────────────────────────────────

    #[test]
    fn announce_block_serde_round_trip() {
        let ann = AnnounceBlock {
            height: 100,
            hash: "aabb".to_string(),
            prev: "ccdd".to_string(),
        };
        let bytes = bincode::serialize(&ann).unwrap();
        let rt: AnnounceBlock = bincode::deserialize(&bytes).unwrap();
        assert_eq!(rt.height, 100);
        assert_eq!(rt.hash, "aabb");
        assert_eq!(rt.prev, "ccdd");
    }
}

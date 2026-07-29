use crate::config::constants::BLOCK_VERSION;
use serde::{Deserialize, Serialize};

/// Block header — the consensus-committed summary of one block.
///
/// Every field is consensus-critical: any mutation invalidates the PoW hash.
/// The ordering and byte layout are fixed by `canonical_bytes`; do not
/// reorder fields without bumping `BLOCK_VERSION`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockHeader {
    /// Hash of the parent block (64 hex chars; all-zeros for genesis).
    pub parent_hash: String,

    /// Block height (0 = genesis).
    pub number: u64,

    /// Unix timestamp (seconds) when the block was produced.
    pub timestamp: u64,

    /// PoW difficulty target at the time this block was mined.
    pub difficulty: u64,

    /// PoW nonce found by the miner.
    pub nonce: u64,

    /// Hex-encoded VisionX PoW hash that satisfies `difficulty`.
    pub pow_hash: String,

    /// Merkle root of all state after this block is applied.
    pub state_root: String,

    /// Merkle root of all transactions in this block.
    pub tx_root: String,

    /// Address of the miner that produced this block (reward recipient).
    pub miner: String,
}

impl BlockHeader {
    /// Canonical byte encoding fed to the VisionX hash function.
    ///
    /// Fixed 100-byte layout:
    ///   `BLOCK_VERSION(4) | height(8) | parent_hash(32) | timestamp(8)
    ///    | difficulty(8) | nonce(8) | tx_root(32)`
    ///
    /// This layout is identical to `miner::job::MiningJob::encode_header`.
    /// If you change this, bump `BLOCK_VERSION` and update genesis.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(100);
        buf.extend_from_slice(&BLOCK_VERSION.to_be_bytes());
        buf.extend_from_slice(&self.number.to_be_bytes());
        buf.extend_from_slice(&decode_hash_32(&self.parent_hash));
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.difficulty.to_be_bytes());
        buf.extend_from_slice(&self.nonce.to_be_bytes());
        buf.extend_from_slice(&decode_hash_32(&self.tx_root));
        buf
    }

    /// Compute the PoW hash of this header (blake3 of `canonical_bytes`).
    ///
    /// For a mined block `self.pow_hash == self.compute_hash()` must hold.
    /// Use this to verify PoW claims without calling the full VisionX DAG.
    pub fn compute_hash(&self) -> String {
        hex::encode(blake3::hash(&self.canonical_bytes()).as_bytes())
    }
}

/// Decode a hex hash string into a fixed 32-byte array, zero-padding on error.
fn decode_hash_32(hex_str: &str) -> [u8; 32] {
    let raw = hex::decode(hex_str).unwrap_or_else(|_| vec![0u8; 32]);
    let mut out = [0u8; 32];
    let len = raw.len().min(32);
    out[..len].copy_from_slice(&raw[..len]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> BlockHeader {
        BlockHeader {
            parent_hash: "ab".repeat(32),
            number: 1,
            timestamp: 1_700_000_000,
            difficulty: 1_000,
            nonce: 42,
            pow_hash: "cd".repeat(32),
            state_root: "ef".repeat(32),
            tx_root: "01".repeat(32),
            miner: "0xdeadbeef".to_string(),
        }
    }

    #[test]
    fn header_serde_round_trip() {
        let h = sample_header();
        let json = serde_json::to_string(&h).unwrap();
        let h2: BlockHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn canonical_bytes_fixed_length() {
        assert_eq!(sample_header().canonical_bytes().len(), 100);
    }

    #[test]
    fn canonical_bytes_is_deterministic() {
        let h = sample_header();
        assert_eq!(h.canonical_bytes(), h.canonical_bytes());
    }

    #[test]
    fn compute_hash_is_deterministic() {
        let h = sample_header();
        assert_eq!(h.compute_hash(), h.compute_hash());
    }

    #[test]
    fn compute_hash_is_hex_64_chars() {
        let hash = sample_header().compute_hash();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn nonce_changes_hash() {
        let mut h = sample_header();
        let h1 = h.compute_hash();
        h.nonce = 9_999;
        let h2 = h.compute_hash();
        assert_ne!(h1, h2);
    }

    #[test]
    fn genesis_header_compute_hash_matches_constant() {
        use crate::genesis::genesis::{genesis_block, GENESIS_HASH};
        let blk = genesis_block();
        assert_eq!(
            blk.header.compute_hash(),
            GENESIS_HASH,
            "genesis BlockHeader::compute_hash() must equal the locked GENESIS_HASH constant"
        );
    }
}

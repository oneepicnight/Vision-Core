use serde::{Deserialize, Serialize};
use crate::types::{BlockHeader, Tx};

/// A complete block: header + ordered transactions + serialised weight.
///
/// All blocks — whether received from peers, produced locally, or loaded
/// from storage — use this single representation. There is no alternate
/// block type in the codebase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    /// Consensus-committed header (PoW hash, state root, etc.).
    pub header: BlockHeader,

    /// Ordered list of transactions included in this block.
    pub txs: Vec<Tx>,

    /// Serialised weight of the block (sum of per-tx weights).
    /// Must not exceed `BLOCK_WEIGHT_LIMIT`.
    pub weight: u64,
}

impl Block {
    /// Canonical block hash — the hex-encoded PoW hash from the header.
    #[inline]
    pub fn hash(&self) -> &str {
        &self.header.pow_hash
    }

    /// Block height, forwarded from header.
    #[inline]
    pub fn height(&self) -> u64 {
        self.header.number
    }

    /// Compute the tx_root from the included transactions.
    ///
    /// Feeds each `tx.tx_id()` into a blake3 hasher in order. Returns
    /// the null hash (`"00...00"`, 64 chars) for an empty block.
    /// The block's `header.tx_root` must equal this value for the block
    /// to be considered internally consistent.
    pub fn compute_tx_root(&self) -> String {
        if self.txs.is_empty() {
            return "0".repeat(64);
        }
        let mut hasher = blake3::Hasher::new();
        for tx in &self.txs {
            hasher.update(tx.tx_id().as_bytes());
        }
        hex::encode(hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tx;

    fn sample_block() -> Block {
        Block {
            header: BlockHeader {
                parent_hash: "00".repeat(32),
                number:      1,
                timestamp:   1_700_000_001,
                difficulty:  1_000,
                nonce:       7,
                pow_hash:    "ab".repeat(32),
                state_root:  "cd".repeat(32),
                tx_root:     "0".repeat(64),
                miner:       "0xminer".to_string(),
            },
            txs:    vec![],
            weight: 0,
        }
    }

    #[test]
    fn block_serde_round_trip() {
        let b = sample_block();
        let json = serde_json::to_string(&b).unwrap();
        let b2: Block = serde_json::from_str(&json).unwrap();
        assert_eq!(b, b2);
    }

    #[test]
    fn block_hash_equals_pow_hash() {
        let b = sample_block();
        assert_eq!(b.hash(), b.header.pow_hash.as_str());
    }

    #[test]
    fn empty_block_tx_root_is_null_hash() {
        let b = sample_block();
        assert_eq!(b.compute_tx_root(), "0".repeat(64));
    }

    #[test]
    fn tx_root_changes_with_transactions() {
        let mut b = sample_block();
        let root_empty = b.compute_tx_root();
        b.txs.push(Tx {
            nonce:        0,
            sender_pubkey: "aa".repeat(32),
            module:       "transfer".to_string(),
            method:       "send".to_string(),
            args:         vec![1, 2, 3],
            tip:          0,
            fee_limit:    1_000,
            sig:          "sig".to_string(),
        });
        let root_with_tx = b.compute_tx_root();
        assert_ne!(root_empty, root_with_tx);
    }
}

use serde::{Deserialize, Serialize};

/// A signed transaction submitted by a user or relayed by a peer.
///
/// Transactions reference a `module` + `method` pair so the execution layer
/// dispatches to typed logic. No EVM opcodes, no access lists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tx {
    /// Per-sender monotonic counter; prevents replay attacks.
    pub nonce: u64,

    /// Hex-encoded ed25519 public key of the sender.
    /// Empty string for coinbase/reward transactions.
    pub sender_pubkey: String,

    /// Target module name (e.g. `"transfer"`, `"coinbase"`).
    pub module: String,

    /// Method within the module (e.g. `"send"`, `"reward"`).
    pub method: String,

    /// Serialised arguments for `module::method`.
    pub args: Vec<u8>,

    /// Extra fee tip paid to the miner (raw token units).
    pub tip: u64,

    /// Maximum total fee the sender authorises (raw token units).
    pub fee_limit: u64,

    /// Ed25519 signature over the canonical serialised fields (base64).
    /// Empty string for coinbase/reward transactions.
    pub sig: String,
}

impl Tx {
    /// Canonical transaction id: blake3 hex of the bincode-serialised Tx.
    pub fn tx_id(&self) -> String {
        let bytes = bincode::serialize(self).unwrap_or_default();
        hex::encode(blake3::hash(&bytes).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tx() -> Tx {
        Tx {
            nonce:        1,
            sender_pubkey: "aa".repeat(32),
            module:       "transfer".to_string(),
            method:       "send".to_string(),
            args:         vec![0xde, 0xad, 0xbe, 0xef],
            tip:          100,
            fee_limit:    10_000,
            sig:          "base64sighere".to_string(),
        }
    }

    fn coinbase_tx(height: u64) -> Tx {
        Tx {
            nonce:        height,
            sender_pubkey: String::new(),
            module:       "coinbase".to_string(),
            method:       "reward".to_string(),
            args:         height.to_be_bytes().to_vec(),
            tip:          0,
            fee_limit:    0,
            sig:          String::new(),
        }
    }

    #[test]
    fn tx_serde_round_trip() {
        let tx = sample_tx();
        let json = serde_json::to_string(&tx).unwrap();
        let tx2: Tx = serde_json::from_str(&json).unwrap();
        assert_eq!(tx, tx2);
    }

    #[test]
    fn tx_id_is_deterministic() {
        let tx = sample_tx();
        assert_eq!(tx.tx_id(), tx.tx_id());
    }

    #[test]
    fn tx_id_is_hex_64_chars() {
        let id = sample_tx().tx_id();
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn tx_id_changes_with_nonce() {
        let mut tx = sample_tx();
        let id1 = tx.tx_id();
        tx.nonce = 99;
        let id2 = tx.tx_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn coinbase_tx_has_empty_sender_and_sig() {
        let cb = coinbase_tx(100);
        assert!(cb.sender_pubkey.is_empty());
        assert!(cb.sig.is_empty());
        assert_eq!(cb.module, "coinbase");
        assert_eq!(cb.method, "reward");
    }

    #[test]
    fn coinbase_tx_id_deterministic_per_height() {
        let cb1 = coinbase_tx(100);
        let cb2 = coinbase_tx(100);
        let cb3 = coinbase_tx(101);
        assert_eq!(cb1.tx_id(), cb2.tx_id());
        assert_ne!(cb1.tx_id(), cb3.tx_id());
    }
}

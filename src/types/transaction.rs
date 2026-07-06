use serde::{Deserialize, Serialize};

/// A signed transaction submitted by a user or relayed by a peer.
///
/// Transactions reference a `module` + `method` pair so the execution layer
/// dispatches to typed logic. No EVM opcodes, no access lists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tx {
    /// Per-sender monotonic counter; prevents replay attacks.
    pub nonce: u64,

    /// Hex-encoded Ed25519 public key of the sender.
    /// Empty string for coinbase/reward transactions.
    pub sender_pubkey: String,

    /// Target module name (e.g. `"cash"`, `"coinbase"`).
    pub module: String,

    /// Method within the module (e.g. `"transfer"`, `"reward"`).
    pub method: String,

    /// Serialised arguments for `module::method`.
    pub args: Vec<u8>,

    /// Extra fee tip paid to the miner (raw token units).
    pub tip: u64,

    /// Maximum total fee the sender authorises (raw token units).
    pub fee_limit: u64,

    /// Hex-encoded Ed25519 signature over the canonical unsigned payload.
    /// Empty string for coinbase/reward transactions.
    pub sig: String,
}

/// Serialize `tx` with `sig` cleared.
///
/// These bytes are the canonical transaction signing payload for Vision-Core.
/// The payload deliberately excludes the signature while preserving all other
/// fields in the clean `Tx` envelope.
pub fn canonical_unsigned_payload(tx: &Tx) -> Vec<u8> {
    let mut unsigned = tx.clone();
    unsigned.sig.clear();
    bincode::serialize(&unsigned).expect("serializing Tx to bincode should not fail")
}

/// Canonical transaction id: BLAKE3 over the canonical unsigned payload.
pub fn canonical_tx_id(tx: &Tx) -> String {
    let payload = canonical_unsigned_payload(tx);
    hex::encode(blake3::hash(&payload).as_bytes())
}

impl Tx {
    /// Compatibility transaction id currently used by existing block and
    /// mempool paths.
    ///
    /// New transaction validation work should use `canonical_tx_id` until the
    /// consensus call sites are explicitly migrated in a later commit.
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
            module:       "cash".to_string(),
            method:       "transfer".to_string(),
            args:         vec![0xde, 0xad, 0xbe, 0xef],
            tip:          100,
            fee_limit:    10_000,
            sig:          "11".repeat(64),
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

    const EXPECTED_CANONICAL_PAYLOAD_HEX: &str = concat!(
        "0100000000000000",
        "4000000000000000",
        "61616161616161616161616161616161",
        "61616161616161616161616161616161",
        "61616161616161616161616161616161",
        "61616161616161616161616161616161",
        "0400000000000000",
        "63617368",
        "0800000000000000",
        "7472616e73666572",
        "0400000000000000",
        "deadbeef",
        "6400000000000000",
        "1027000000000000",
        "0000000000000000",
    );

    const EXPECTED_CANONICAL_TX_ID: &str =
        "a7fc34bf3332fec96623ea7f5ddb638aaad51f039091d2d5bf94adb76a26f0dd";

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
    fn canonical_unsigned_payload_matches_test_vector() {
        let tx = sample_tx();
        let payload = canonical_unsigned_payload(&tx);
        assert_eq!(hex::encode(payload), EXPECTED_CANONICAL_PAYLOAD_HEX);
    }

    #[test]
    fn canonical_unsigned_payload_is_deterministic() {
        let tx = sample_tx();
        assert_eq!(
            canonical_unsigned_payload(&tx),
            canonical_unsigned_payload(&tx),
        );
    }

    #[test]
    fn canonical_tx_id_matches_test_vector() {
        let tx = sample_tx();
        assert_eq!(canonical_tx_id(&tx), EXPECTED_CANONICAL_TX_ID);
    }

    #[test]
    fn canonical_tx_id_is_deterministic() {
        let tx = sample_tx();
        assert_eq!(canonical_tx_id(&tx), canonical_tx_id(&tx));
    }

    #[test]
    fn canonical_tx_id_unchanged_when_only_sig_changes() {
        let mut tx = sample_tx();
        let id1 = canonical_tx_id(&tx);
        tx.sig = "22".repeat(64);
        let id2 = canonical_tx_id(&tx);
        assert_eq!(id1, id2);
    }

    #[test]
    fn canonical_tx_id_changes_when_nonce_changes() {
        let mut tx = sample_tx();
        let id1 = canonical_tx_id(&tx);
        tx.nonce += 1;
        let id2 = canonical_tx_id(&tx);
        assert_ne!(id1, id2);
    }

    #[test]
    fn canonical_tx_id_changes_when_args_change() {
        let mut tx = sample_tx();
        let id1 = canonical_tx_id(&tx);
        tx.args.push(0x42);
        let id2 = canonical_tx_id(&tx);
        assert_ne!(id1, id2);
    }

    #[test]
    fn canonical_tx_id_changes_when_tip_changes() {
        let mut tx = sample_tx();
        let id1 = canonical_tx_id(&tx);
        tx.tip += 1;
        let id2 = canonical_tx_id(&tx);
        assert_ne!(id1, id2);
    }

    #[test]
    fn canonical_tx_id_changes_when_fee_limit_changes() {
        let mut tx = sample_tx();
        let id1 = canonical_tx_id(&tx);
        tx.fee_limit += 1;
        let id2 = canonical_tx_id(&tx);
        assert_ne!(id1, id2);
    }

    #[test]
    fn canonical_payload_excludes_sig() {
        let mut tx = sample_tx();
        let payload1 = canonical_unsigned_payload(&tx);
        tx.sig = "ff".repeat(64);
        let payload2 = canonical_unsigned_payload(&tx);

        assert_eq!(payload1, payload2);
        assert!(!hex::encode(payload1).contains(&"11".repeat(64)));
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

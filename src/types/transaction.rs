use ed25519_dalek::{Signature, Verifier, VerifyingKey};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxSignatureError {
    SenderPubkeyWrongLength,
    SenderPubkeyNotLowercaseHex,
    SignatureWrongLength,
    SignatureNotLowercaseHex,
    MalformedPublicKey,
    InvalidSignature,
}

fn is_lowercase_hex(s: &str) -> bool {
    s.as_bytes()
        .iter()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn decode_sender_pubkey(sender_pubkey: &str) -> Result<[u8; 32], TxSignatureError> {
    if sender_pubkey.len() != 64 {
        return Err(TxSignatureError::SenderPubkeyWrongLength);
    }
    if !is_lowercase_hex(sender_pubkey) {
        return Err(TxSignatureError::SenderPubkeyNotLowercaseHex);
    }

    let bytes =
        hex::decode(sender_pubkey).map_err(|_| TxSignatureError::SenderPubkeyNotLowercaseHex)?;
    bytes
        .try_into()
        .map_err(|_| TxSignatureError::SenderPubkeyWrongLength)
}

fn decode_signature(sig: &str) -> Result<[u8; 64], TxSignatureError> {
    if sig.len() != 128 {
        return Err(TxSignatureError::SignatureWrongLength);
    }
    if !is_lowercase_hex(sig) {
        return Err(TxSignatureError::SignatureNotLowercaseHex);
    }

    let bytes = hex::decode(sig).map_err(|_| TxSignatureError::SignatureNotLowercaseHex)?;
    bytes
        .try_into()
        .map_err(|_| TxSignatureError::SignatureWrongLength)
}

/// Verify the hex-encoded Ed25519 transaction signature.
///
/// The signature must verify over `canonical_unsigned_payload(tx)`, which is
/// bincode serialization of the clean `Tx` envelope with `sig` cleared.
pub fn verify_tx_signature(tx: &Tx) -> Result<(), TxSignatureError> {
    let pubkey_bytes = decode_sender_pubkey(&tx.sender_pubkey)?;
    let sig_bytes = decode_signature(&tx.sig)?;

    let verifying_key = VerifyingKey::from_bytes(&pubkey_bytes)
        .map_err(|_| TxSignatureError::MalformedPublicKey)?;
    let signature = Signature::from_bytes(&sig_bytes);

    verifying_key
        .verify(&canonical_unsigned_payload(tx), &signature)
        .map_err(|_| TxSignatureError::InvalidSignature)
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
    use ed25519_dalek::{Signer, SigningKey};

    fn sample_tx() -> Tx {
        Tx {
            nonce: 1,
            sender_pubkey: "aa".repeat(32),
            module: "cash".to_string(),
            method: "transfer".to_string(),
            args: vec![0xde, 0xad, 0xbe, 0xef],
            tip: 100,
            fee_limit: 10_000,
            sig: "11".repeat(64),
        }
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn signed_sample_tx() -> Tx {
        let signing_key = signing_key(7);
        let mut tx = sample_tx();
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();

        let sig = signing_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }
    fn malformed_public_key_hex() -> String {
        for candidate in 0u16..=u16::MAX {
            let mut bytes = [0u8; 32];
            bytes[0] = candidate as u8;
            bytes[1] = (candidate >> 8) as u8;

            if ed25519_dalek::VerifyingKey::from_bytes(&bytes).is_err() {
                return hex::encode(bytes);
            }
        }

        panic!("expected at least one malformed Ed25519 public key candidate");
    }
    fn coinbase_tx(height: u64) -> Tx {
        Tx {
            nonce: height,
            sender_pubkey: String::new(),
            module: "coinbase".to_string(),
            method: "reward".to_string(),
            args: height.to_be_bytes().to_vec(),
            tip: 0,
            fee_limit: 0,
            sig: String::new(),
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
    fn tx_signature_accepts_valid_ed25519_signature() {
        let tx = signed_sample_tx();
        assert_eq!(verify_tx_signature(&tx), Ok(()));
    }

    #[test]
    fn tx_signature_rejects_sender_pubkey_wrong_length() {
        let mut tx = signed_sample_tx();
        tx.sender_pubkey = "aa".repeat(31);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::SenderPubkeyWrongLength),
        );
    }

    #[test]
    fn tx_signature_rejects_sender_pubkey_not_lowercase_hex() {
        let mut tx = signed_sample_tx();
        tx.sender_pubkey = "AA".repeat(32);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::SenderPubkeyNotLowercaseHex),
        );

        tx.sender_pubkey = "gg".repeat(32);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::SenderPubkeyNotLowercaseHex),
        );
    }

    #[test]
    fn tx_signature_rejects_malformed_public_key() {
        let mut tx = signed_sample_tx();
        tx.sender_pubkey = malformed_public_key_hex();
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::MalformedPublicKey),
        );
    }

    #[test]
    fn tx_signature_rejects_signature_wrong_length() {
        let mut tx = signed_sample_tx();
        tx.sig = "11".repeat(63);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::SignatureWrongLength),
        );
    }

    #[test]
    fn tx_signature_rejects_signature_not_lowercase_hex() {
        let mut tx = signed_sample_tx();
        tx.sig = "AA".repeat(64);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::SignatureNotLowercaseHex),
        );

        tx.sig = "gg".repeat(64);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::SignatureNotLowercaseHex),
        );
    }

    #[test]
    fn tx_signature_rejects_signature_from_wrong_key() {
        let good_key = signing_key(7);
        let wrong_key = signing_key(8);
        let mut tx = sample_tx();
        tx.sender_pubkey = hex::encode(wrong_key.verifying_key().to_bytes());
        tx.sig.clear();

        let sig = good_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());

        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );
    }

    #[test]
    fn tx_signature_rejects_tampered_signed_fields() {
        let base = signed_sample_tx();

        let mut tx = base.clone();
        tx.nonce += 1;
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );

        let mut tx = base.clone();
        tx.module = "stake".to_string();
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );

        let mut tx = base.clone();
        tx.method = "lock".to_string();
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );

        let mut tx = base.clone();
        tx.args.push(0x42);
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );

        let mut tx = base.clone();
        tx.tip += 1;
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );

        let mut tx = base;
        tx.fee_limit += 1;
        assert_eq!(
            verify_tx_signature(&tx),
            Err(TxSignatureError::InvalidSignature),
        );
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

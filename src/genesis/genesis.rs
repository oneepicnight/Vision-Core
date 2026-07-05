use std::collections::BTreeMap;
use anyhow::{anyhow, Result};
use crate::config::constants::DIFFICULTY_FLOOR;
use crate::types::{Block, BlockHeader};

// ─── Consensus-locked hashes ──────────────────────────────────────────────────

/// Canonical PoW hash of the genesis block.
///
/// Produced by `genesis_block().header.compute_hash()` — i.e., blake3 of
/// `BlockHeader::canonical_bytes()` with the parameters below.
///
/// DO NOT change — changing this value is a hard fork.
pub const GENESIS_HASH: &str =
    "d6469ec95f56b56be4921ef40b9795902c96f2ad26582ef8db8fac46f4a7aa13";

/// Economics fingerprint: blake3 over each vault's 20-byte address followed
/// by its 4-byte big-endian BPS share, concatenated in declaration order.
///
/// DO NOT change — changing this value is a hard fork.
pub const ECON_HASH: &str =
    "a18f9f82aeb6276b5cfb353e351cd0cf9b34aad962e29f4ac6268f0659c55f95";

// ─── Genesis block parameters ─────────────────────────────────────────────────
//
// These values are fixed forever. Changing any of them changes GENESIS_HASH
// and therefore represents a hard fork.

/// Height of the genesis block.
const GENESIS_HEIGHT: u64 = 0;

/// Unix timestamp of the genesis block (network epoch, not wall clock).
const GENESIS_TIMESTAMP: u64 = 0;

/// Initial PoW difficulty (= DIFFICULTY_FLOOR; retarget starts from here).
const GENESIS_DIFFICULTY: u64 = DIFFICULTY_FLOOR;

/// PoW nonce used in the genesis block.
const GENESIS_NONCE: u64 = 0;

/// Miner address recorded on the genesis block (no reward is paid here).
const GENESIS_MINER: &str = "network_genesis";

// ─── Vault accounts (economics) ───────────────────────────────────────────────
//
// These accounts receive the split of block rewards and burned fees that are
// redirected to the protocol treasury. Their addresses and basis-point shares
// are hashed into ECON_HASH so any peer with mismatched economics is rejected.
//
// Encoding (must match vision-node chain::economics::econ_hash):
//   1. Each address string (with 0x prefix, lowercase) fed as raw UTF-8 bytes.
//   2. Each BPS value fed as u32 little-endian — in the same order as the addresses.

/// ("0x-prefixed address", basis_points) pairs in canonical order.
/// Total must equal 10_000 bps (100 %).
const VAULT_ACCOUNTS: &[(&str, u32)] = &[
    ("0xb977c16e539670ddfecc0ac902fcb916ec4b944e", 5_000), // staking    50 %
    ("0x8bb8edcd4cdbcb132cc5e88ff90ba48cebf11cbd", 3_000), // ecosystem  30 %
    ("0xdf7a79291bb96e9dd1c77da089933767999eabf0", 1_000), // founder1   10 %
    ("0x083f95edd48e3e9da396891b704994b86e7790e7", 1_000), // founder2   10 %
];

// ─── Hash computation ─────────────────────────────────────────────────────────

/// Compute the genesis block PoW hash deterministically.
///
/// Delegates to `genesis_block().header.compute_hash()` so the encoding is
/// defined in exactly one place (`BlockHeader::canonical_bytes`).
pub fn compute_genesis_pow_hash() -> String {
    genesis_block().header.compute_hash()
}

/// Compute the economics fingerprint deterministically.
///
/// Encoding matches `vision-node::chain::economics::econ_hash`:
/// 1. Each vault address string (with `0x` prefix, lowercase) as raw UTF-8.
/// 2. Each vault BPS share as `u32` little-endian — in declaration order.
pub fn compute_econ_hash() -> String {
    let mut hasher = blake3::Hasher::new();
    // Pass 1: all address strings.
    for (addr, _) in VAULT_ACCOUNTS {
        hasher.update(addr.as_bytes());
    }
    // Pass 2: all BPS values in LE.
    for (_, bps) in VAULT_ACCOUNTS {
        hasher.update(&bps.to_le_bytes());
    }
    hex::encode(hasher.finalize().as_bytes())
}

// ─── Validation ───────────────────────────────────────────────────────────────

/// Validate that the computed genesis hash matches the canonical constant.
///
/// MUST succeed before the node starts. Failure means the binary or constants
/// have been tampered with.
pub fn validate_genesis_hash() -> Result<()> {
    let computed = compute_genesis_pow_hash();
    if computed != GENESIS_HASH {
        return Err(anyhow!(
            "Genesis hash mismatch — expected {GENESIS_HASH}, computed {computed}. \
             Verify binary integrity."
        ));
    }
    tracing::info!("Genesis hash OK: {}", GENESIS_HASH);
    Ok(())
}

/// Validate that the computed economics hash matches the canonical constant.
///
/// MUST succeed before the node starts. Failure means a vault address or BPS
/// share has been changed without a coordinated hard fork.
pub fn validate_econ_hash() -> Result<()> {
    let computed = compute_econ_hash();
    if computed != ECON_HASH {
        return Err(anyhow!(
            "Econ hash mismatch — expected {ECON_HASH}, computed {computed}. \
             Verify vault accounts in genesis.rs."
        ));
    }
    tracing::info!("Econ hash OK: {}", ECON_HASH);
    Ok(())
}

/// Validate that a stored genesis hash matches the canonical constant.
///
/// Call during DB open to guard against cross-network database contamination.
pub fn verify_stored_genesis(stored: &str) -> Result<()> {
    if stored != GENESIS_HASH {
        return Err(anyhow!(
            "Stored genesis mismatch — expected {GENESIS_HASH}, found {stored}. \
             Delete the data directory and restart."
        ));
    }
    Ok(())
}

/// Reject a peer whose genesis hash does not match ours.
///
/// Called during handshake. Different genesis = different network.
pub fn verify_peer_genesis(peer_genesis: &str) -> Result<()> {
    if peer_genesis != GENESIS_HASH {
        return Err(anyhow!(
            "Peer genesis mismatch ({peer_genesis}) — dropping connection."
        ));
    }
    Ok(())
}

// ─── Block construction ───────────────────────────────────────────────────────

/// Construct the genesis block.
///
/// The genesis block is never mined. Its PoW hash is the deterministic blake3
/// commitment of its canonical header bytes via `BlockHeader::canonical_bytes`.
/// The result must equal `GENESIS_HASH` — verified by `validate_genesis_hash`.
pub fn genesis_block() -> Block {
    let null_hash = "0".repeat(64);
    Block {
        header: BlockHeader {
            parent_hash: null_hash.clone(),
            number:      GENESIS_HEIGHT,
            timestamp:   GENESIS_TIMESTAMP,
            difficulty:  GENESIS_DIFFICULTY,
            nonce:       GENESIS_NONCE,
            pow_hash:    GENESIS_HASH.to_string(),
            state_root:  null_hash.clone(),
            tx_root:     null_hash,
            miner:       GENESIS_MINER.to_string(),
        },
        txs:    vec![],
        weight: 0,
    }
}

// ─── Initial chain state ──────────────────────────────────────────────────────

/// Return the initial account balances that must be present when a fresh chain
/// is bootstrapped from genesis.
///
/// Vision Core uses pure-emission tokenomics: all supply is minted through
/// block rewards. No addresses receive a pre-mine balance at genesis, so this
/// map is empty. The function exists as the single documented place where the
/// genesis balances are defined; add entries here if a pre-mine is ever needed.
pub fn genesis_balances() -> BTreeMap<String, u128> {
    BTreeMap::new()
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Genesis hash ──────────────────────────────────────────────────────────

    #[test]
    fn genesis_hash_is_deterministic() {
        assert_eq!(compute_genesis_pow_hash(), compute_genesis_pow_hash());
    }

    #[test]
    fn genesis_block_compute_hash_matches_constant() {
        assert_eq!(
            genesis_block().header.compute_hash(),
            GENESIS_HASH,
            "genesis block header hash must equal the locked GENESIS_HASH constant"
        );
    }

    #[test]
    fn genesis_pow_hash_function_matches_constant() {
        assert_eq!(compute_genesis_pow_hash(), GENESIS_HASH);
    }

    #[test]
    fn validate_genesis_hash_succeeds() {
        validate_genesis_hash().expect("validate_genesis_hash must not fail");
    }

    // ── Genesis block shape ───────────────────────────────────────────────────

    #[test]
    fn genesis_block_height_is_zero() {
        assert_eq!(genesis_block().height(), 0);
    }

    #[test]
    fn genesis_block_has_no_transactions() {
        assert!(genesis_block().txs.is_empty());
    }

    #[test]
    fn genesis_block_weight_is_zero() {
        assert_eq!(genesis_block().weight, 0);
    }

    #[test]
    fn genesis_block_parent_is_null_hash() {
        assert_eq!(genesis_block().header.parent_hash, "0".repeat(64));
    }

    #[test]
    fn genesis_block_difficulty_equals_floor() {
        assert_eq!(genesis_block().header.difficulty, DIFFICULTY_FLOOR);
    }

    #[test]
    fn genesis_block_tx_root_is_null_hash() {
        // Empty block — compute_tx_root must equal the null hash placeholder.
        assert_eq!(genesis_block().compute_tx_root(), "0".repeat(64));
        assert_eq!(genesis_block().header.tx_root, "0".repeat(64));
    }

    #[test]
    fn genesis_block_is_internally_consistent() {
        let b = genesis_block();
        // The stored pow_hash must match what compute_hash() would produce.
        assert_eq!(b.hash(), b.header.compute_hash().as_str());
        // tx_root in the header must match the actual tx list.
        assert_eq!(b.header.tx_root, b.compute_tx_root());
    }

    // ── Economics hash ────────────────────────────────────────────────────────

    #[test]
    fn econ_hash_is_deterministic() {
        assert_eq!(compute_econ_hash(), compute_econ_hash());
    }

    #[test]
    fn econ_hash_matches_constant() {
        assert_eq!(
            compute_econ_hash(),
            ECON_HASH,
            "compute_econ_hash() must match the locked ECON_HASH constant; \
             if the encoding has changed, update ECON_HASH after network coordination"
        );
    }

    #[test]
    fn validate_econ_hash_succeeds() {
        validate_econ_hash().expect("validate_econ_hash must not fail");
    }

    #[test]
    fn econ_hash_is_hex_64_chars() {
        let h = compute_econ_hash();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn vault_bps_sum_to_ten_thousand() {
        let total: u32 = VAULT_ACCOUNTS.iter().map(|(_, bps)| bps).sum();
        assert_eq!(total, 10_000, "vault BPS shares must sum to 10_000 (100 %)");
    }

    #[test]
    fn vault_addresses_are_valid_hex_20_bytes() {
        for (addr, _) in VAULT_ACCOUNTS {
            let stripped = addr.trim_start_matches("0x");
            let decoded = hex::decode(stripped)
                .unwrap_or_else(|_| panic!("vault address is not valid hex: {addr}"));
            assert_eq!(decoded.len(), 20, "vault address must be 20 bytes: {addr}");
        }
    }

    // ── Peer / storage verification ───────────────────────────────────────────

    #[test]
    fn verify_stored_genesis_accepts_correct_hash() {
        verify_stored_genesis(GENESIS_HASH).expect("correct hash must be accepted");
    }

    #[test]
    fn verify_stored_genesis_rejects_wrong_hash() {
        assert!(verify_stored_genesis("deadbeef").is_err());
    }

    #[test]
    fn verify_peer_genesis_accepts_correct_hash() {
        verify_peer_genesis(GENESIS_HASH).expect("correct genesis must be accepted");
    }

    #[test]
    fn verify_peer_genesis_rejects_wrong_hash() {
        assert!(verify_peer_genesis("00000000").is_err());
    }

    // ── Initial chain state ───────────────────────────────────────────────────

    #[test]
    fn genesis_balances_is_empty_at_launch() {
        assert!(genesis_balances().is_empty(),
            "pure-emission chain has no pre-mine; genesis balances must be empty");
    }
}

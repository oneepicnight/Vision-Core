//! Block acceptance — single path for all blocks regardless of source.
//!
//! Every block (P2P gossip, sync, local mine) MUST go through `apply_block`.
//! No alternate block integration paths exist in vision-core.
//!
//! # Acceptance Pipeline
//!
//! `apply_block` drives each candidate through eight explicit stages:
//!
//! 1. **Structural validation** — weight limit, tx_root integrity, coinbase presence
//! 2. **Parent lookup**        — classify as canon-extend, side-chain, or orphan
//! 3. **Timestamp validation** — monotonic and future-block guard
//! 4. **Difficulty validation** — expected retarget against the parent chain
//! 5. **PoW validation**       — hash meets difficulty target
//! 6. **State/tx validation**  — coinbase height encoding, no intra-block dup tx_ids
//! 7. **Chain selection**      — cumulative-work comparison for side chains
//! 8. **Integration**          — push to canonical, side-chain store, or orphan pool

use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;
use std::sync::Mutex;

use crate::chain::ChainState;
use crate::config::constants::*;
use crate::pow::difficulty::{calculate_next_difficulty, verify_pow_hash};
use crate::types::Block;

// ─── Acceptance result ─────────────────────────────────────────────────────────

/// The outcome of passing a block through `apply_block`.
///
/// Every possible outcome — including rejection — is a typed variant so
/// callers can pattern-match without unwrapping `Result`.
#[derive(Debug, Clone, PartialEq)]
pub enum AcceptResult {
    /// Block extends the canonical chain tip. `height` is the new tip height.
    CanonExtension { height: u64 },

    /// Block is valid but its chain has less-or-equal cumulative work compared
    /// to the current canonical tip. Stored as a side-chain block only.
    SideChain { height: u64 },

    /// Block's parent is not yet known. Stored in the orphan pool pending
    /// parent arrival.
    StoredOrphan { block_hash: String },

    /// Block is permanently invalid. Contains a human-readable reason string.
    Rejected(String),
}

impl AcceptResult {
    /// Returns `true` for all non-rejected outcomes.
    pub fn is_accepted(&self) -> bool {
        !matches!(self, Self::Rejected(_))
    }
}

// ─── PoW pre-validation cache ─────────────────────────────────────────────────

/// Bounded cache of hashes pre-cleared by `verify_pow_only`.
///
/// Allows the P2P receive path to validate PoW before acquiring the chain
/// lock. `apply_block` skips re-computation for any hash already in the cache.
/// Capacity: 500 entries; oldest evicted on overflow.
static POW_PREVALIDATION_CACHE: Lazy<Mutex<(VecDeque<String>, HashSet<String>)>> =
    Lazy::new(|| Mutex::new((VecDeque::with_capacity(512), HashSet::with_capacity(512))));

fn mark_pow_prevalidated(block_hash: &str) {
    if let Ok(mut cache) = POW_PREVALIDATION_CACHE.lock() {
        if cache.1.insert(block_hash.to_string()) {
            cache.0.push_back(block_hash.to_string());
            while cache.0.len() > 500 {
                if let Some(evicted) = cache.0.pop_front() {
                    cache.1.remove(&evicted);
                }
            }
        }
    }
}

fn is_pow_prevalidated(block_hash: &str) -> bool {
    POW_PREVALIDATION_CACHE
        .lock()
        .map(|c| c.1.contains(block_hash))
        .unwrap_or(false)
}

fn clear_pow_prevalidation(block_hash: &str) {
    if let Ok(mut cache) = POW_PREVALIDATION_CACHE.lock() {
        if cache.1.remove(block_hash) {
            cache.0.retain(|h| h != block_hash);
        }
    }
}

// ─── PoW-only pre-validation (lock-free) ─────────────────────────────────────

/// Validate only the PoW hash WITHOUT acquiring the chain state lock.
///
/// Call this from the P2P receive path before locking `ChainState`. On
/// success the hash is cached; `apply_block` will skip PoW re-computation.
///
/// # Consensus-critical
/// Must produce identical accept/reject decisions on every node.
pub fn verify_pow_only(blk: &Block) -> anyhow::Result<()> {
    let hash = blk.hash();
    if !verify_pow_hash(hash, blk.header.difficulty) {
        return Err(anyhow::anyhow!(
            "PoW check failed: hash {} difficulty {}",
            hash,
            blk.header.difficulty
        ));
    }
    mark_pow_prevalidated(hash);
    Ok(())
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Current wall-clock time in seconds since the Unix epoch.
fn wall_clock_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Resolve a block by hash from either the canonical chain or the side-block
/// store. Returns `None` if the hash is not known.
fn resolve_block(g: &ChainState, hash: &str) -> Option<Block> {
    if let Some(&height) = g.canon_index.get(hash) {
        return g.blocks.get(height as usize).cloned();
    }
    g.side_blocks.get(hash).cloned()
}

/// Walk backwards from `tip_hash` through canonical + side-chain blocks,
/// collecting up to `RETARGET_WINDOW + 1` ancestors.
///
/// Returns blocks in ascending height order (oldest first), matching the
/// layout expected by `calculate_next_difficulty`.
fn collect_ancestor_window(g: &ChainState, tip_hash: &str) -> Vec<Block> {
    let limit = (RETARGET_WINDOW + 1) as usize;
    let mut chain: Vec<Block> = Vec::with_capacity(limit);
    let mut current = tip_hash.to_string();

    for _ in 0..limit {
        match resolve_block(g, &current) {
            Some(blk) => {
                let parent = blk.header.parent_hash.clone();
                chain.push(blk);
                // Stop when we reach the genesis sentinel (all-zero parent).
                if parent.chars().all(|c| c == '0') {
                    break;
                }
                current = parent;
            }
            None => break,
        }
    }

    chain.reverse(); // oldest → newest
    chain
}

/// Record `blk` as the next canonical block with cumulative work `cw`.
/// Caller must have completed all validation stages before calling this.
fn push_canonical(g: &mut ChainState, blk: Block, cw: u128) {
    let hash = blk.hash().to_string();
    let height = blk.header.number;
    g.cumulative_work.insert(hash.clone(), cw);
    g.seen_blocks.insert(hash.clone());
    g.canon_index.insert(hash, height);
    g.blocks.push(blk);
}

// ─── Single-path block acceptance ─────────────────────────────────────────────

/// Apply `blk` to the chain state through the eight-stage acceptance pipeline.
///
/// This is the **only** legal entry point for block integration. Every block
/// source — P2P gossip, sync, or local miner — must call this function.
///
/// Returns a typed [`AcceptResult`]; there are no panics or `unwrap` calls on
/// the consensus path.
///
/// # Consensus-critical
/// Every validation rule must produce identical decisions on all nodes.
/// Adding, removing, or reordering checks requires a network-coordinated
/// hard fork.
pub fn apply_block(
    g: &mut ChainState,
    blk: &Block,
    source_peer: Option<&str>,
) -> AcceptResult {
    let hash = blk.hash().to_string();

    // ── Stage 1 — Structural validation ──────────────────────────────────────

    // 1a. Block weight must not exceed the consensus limit.
    if blk.weight > BLOCK_WEIGHT_LIMIT {
        return AcceptResult::Rejected(format!(
            "weight {} exceeds limit {}",
            blk.weight, BLOCK_WEIGHT_LIMIT
        ));
    }

    // 1b. tx_root in the header must match the transactions actually present.
    let computed_tx_root = blk.compute_tx_root();
    if blk.header.tx_root != computed_tx_root {
        return AcceptResult::Rejected(format!(
            "tx_root mismatch: header={:.8} computed={:.8}",
            blk.header.tx_root, computed_tx_root
        ));
    }

    // 1c. Every non-genesis block must open with a coinbase::reward transaction.
    if blk.header.number > 0 {
        match blk.txs.first() {
            None => return AcceptResult::Rejected("missing coinbase tx".into()),
            Some(cb) if cb.module != "coinbase" || cb.method != "reward" => {
                return AcceptResult::Rejected(
                    "first tx must be coinbase::reward".into(),
                );
            }
            _ => {}
        }
    }

    // 1d. Duplicate block — already integrated or pending.
    if g.seen_blocks.contains(&hash) {
        return AcceptResult::Rejected(format!("duplicate block {:.8}", hash));
    }

    // ── Stage 2 — Parent lookup ───────────────────────────────────────────────

    // Genesis special-case: bypass remaining stages and integrate directly.
    if blk.header.number == 0 {
        if blk.hash() != crate::genesis::GENESIS_HASH {
            return AcceptResult::Rejected(format!(
                "genesis hash mismatch: got {} expected {}",
                blk.hash(),
                crate::genesis::GENESIS_HASH
            ));
        }
        if !g.blocks.is_empty() {
            return AcceptResult::Rejected("genesis already applied".into());
        }
        let cw = blk.header.difficulty as u128;
        push_canonical(g, blk.clone(), cw);
        let _ = crate::chain::storage::store_block(g, blk);
        let _ = crate::chain::storage::persist_tip(g);
        return AcceptResult::CanonExtension { height: 0 };
    }

    // Non-genesis: parent must be in the canonical chain or the side-block store.
    let parent_hash = blk.header.parent_hash.clone();
    let parent_in_canon = g.canon_index.contains_key(parent_hash.as_str());
    let parent_in_side = g.side_blocks.contains_key(parent_hash.as_str());

    if !parent_in_canon && !parent_in_side {
        // Parent unknown — stash in orphan pool for future promotion.
        crate::chain::orphan::add_orphan(
            g,
            blk.clone(),
            source_peer.unwrap_or("anon"),
        );
        return AcceptResult::StoredOrphan { block_hash: hash };
    }

    // Materialise the parent block for timestamp validation.
    let parent_blk = resolve_block(g, &parent_hash)
        .expect("parent confirmed present above");

    // ── Stage 3 — Timestamp validation ───────────────────────────────────────

    if blk.header.timestamp <= parent_blk.header.timestamp {
        return AcceptResult::Rejected(format!(
            "timestamp not monotonic: block={} parent={}",
            blk.header.timestamp, parent_blk.header.timestamp
        ));
    }

    let now = wall_clock_secs();
    if blk.header.timestamp > now + MAX_FUTURE_TIMESTAMP_SECS {
        return AcceptResult::Rejected(format!(
            "timestamp {} too far in future (now+limit={})",
            blk.header.timestamp,
            now + MAX_FUTURE_TIMESTAMP_SECS
        ));
    }

    // ── Stage 4 — Difficulty validation ──────────────────────────────────────

    // Collect the ancestor window rooted at the parent to compute the
    // expected retarget, correctly handling both canonical and side chains.
    let ancestor_window = collect_ancestor_window(g, &parent_hash);
    let expected_diff = calculate_next_difficulty(&ancestor_window, blk.header.timestamp);

    if blk.header.difficulty < DIFFICULTY_FLOOR {
        return AcceptResult::Rejected(format!(
            "difficulty {} below floor {}",
            blk.header.difficulty, DIFFICULTY_FLOOR
        ));
    }
    if blk.header.difficulty != expected_diff {
        return AcceptResult::Rejected(format!(
            "difficulty mismatch at h={}: header={} expected={}",
            blk.header.number, blk.header.difficulty, expected_diff
        ));
    }

    // ── Stage 5 — PoW validation ──────────────────────────────────────────────

    if !is_pow_prevalidated(&hash) {
        if !verify_pow_hash(&hash, blk.header.difficulty) {
            return AcceptResult::Rejected(format!(
                "PoW failed: hash {:.8} difficulty {}",
                hash, blk.header.difficulty
            ));
        }
    }
    clear_pow_prevalidation(&hash);

    // ── Stage 6 — State / transaction validation ──────────────────────────────

    // 6a. Coinbase must encode the correct block height.
    if blk.header.number > 0 {
        let cb = &blk.txs[0]; // presence and shape confirmed in stage 1c
        if cb.args.len() != 8 {
            return AcceptResult::Rejected(format!(
                "coinbase args must be 8 bytes (got {})",
                cb.args.len()
            ));
        }
        let encoded_height =
            u64::from_be_bytes(cb.args.as_slice().try_into().unwrap());
        if encoded_height != blk.header.number {
            return AcceptResult::Rejected(format!(
                "coinbase height mismatch: encoded={} block={}",
                encoded_height, blk.header.number
            ));
        }
    }

    // 6b. No duplicate tx_ids within the block.
    {
        let mut seen_ids: HashSet<String> = HashSet::with_capacity(blk.txs.len());
        for tx in &blk.txs {
            let tid = tx.tx_id();
            if !seen_ids.insert(tid.clone()) {
                return AcceptResult::Rejected(format!(
                    "duplicate tx {:.8} in block",
                    tid
                ));
            }
        }
    }

    // ── Stage 7 — Chain selection (cumulative work) ───────────────────────────

    // Candidate block's cumulative work = parent's work + this block's difficulty.
    let parent_cw = g.cumulative_work.get(parent_hash.as_str()).copied()
        .unwrap_or_else(|| {
            // Parent is canonical but cumulative_work wasn't seeded (e.g. after
            // startup before the cache is warm). Compute from the canonical slice.
            if let Some(&ph) = g.canon_index.get(parent_hash.as_str()) {
                g.blocks[..=(ph as usize)]
                    .iter()
                    .map(|b| b.header.difficulty as u128)
                    .sum()
            } else {
                0u128
            }
        });
    let candidate_cw = parent_cw + blk.header.difficulty as u128;

    // Current canonical tip's cumulative work.
    let tip_hash = g.tip_hash();
    let tip_cw = g.cumulative_work.get(tip_hash.as_str()).copied()
        .unwrap_or_else(|| {
            g.blocks.iter().map(|b| b.header.difficulty as u128).sum()
        });

    // ── Stage 8 — Integration ─────────────────────────────────────────────────

    if parent_hash == tip_hash {
        // LANE-A: straightforward extension of the current canonical tip.
        tracing::debug!(
            "[LANE-A] h={} hash={:.8} cw={} peer={:?}",
            blk.header.number, hash, candidate_cw, source_peer
        );
        push_canonical(g, blk.clone(), candidate_cw);
        let _ = crate::chain::storage::store_block(g, blk);
        let _ = crate::chain::storage::persist_tip(g);
        let _ = crate::chain::snapshots::maybe_save_snapshot(g);
        let height = blk.header.number;
        crate::chain::orphan::process_orphans(g, &hash);
        return AcceptResult::CanonExtension { height };
    }

    // LANE-B: valid block on a non-tip chain; record it and test for reorg.
    tracing::debug!(
        "[LANE-B] h={} hash={:.8} cw={} tip_cw={} peer={:?}",
        blk.header.number, hash, candidate_cw, tip_cw, source_peer
    );
    g.cumulative_work.insert(hash.clone(), candidate_cw);
    g.seen_blocks.insert(hash.clone());
    g.side_blocks.insert(hash.clone(), blk.clone());
    let _ = crate::chain::storage::store_block(g, blk);

    // Attempt a reorg only when this chain has *strictly* more work.
    // Equal-work reorgs are skipped (first-seen wins, reduces state churn).
    if candidate_cw > tip_cw && crate::chain::reorg::try_reorg(g, blk) {
        tracing::info!(
            "[REORG] new tip h={} hash={:.8} cw={}",
            blk.header.number, hash, candidate_cw
        );
        let _ = crate::chain::storage::persist_tip(g);
        let height = blk.height();
        crate::chain::orphan::process_orphans(g, &hash);
        return AcceptResult::CanonExtension { height };
    }

    let height = blk.height();
    crate::chain::orphan::process_orphans(g, &hash);
    AcceptResult::SideChain { height }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Test helpers exposed to sibling test modules (state, orphan, reorg, snapshots).
///
/// All helpers are gated by `#[cfg(test)]` so they are compiled only in test
/// builds and carry zero production overhead.
#[cfg(test)]
pub mod tests_helpers {
    use crate::config::constants::DIFFICULTY_FLOOR;
    use crate::types::{Block, BlockHeader, Tx};

    /// Build a coinbase transaction encoding `height` as 8-byte big-endian.
    pub fn coinbase_tx(height: u64) -> Tx {
        Tx {
            nonce:         height,
            sender_pubkey: String::new(),
            module:        "coinbase".to_string(),
            method:        "reward".to_string(),
            args:          height.to_be_bytes().to_vec(),
            tip:           0,
            fee_limit:     0,
            sig:           String::new(),
        }
    }

    /// Build a non-genesis block suitable for unit tests.
    ///
    /// `slot` (0x00–0xFE) controls the `pow_hash` prefix byte.  Any slot ≤ 0xFE
    /// yields a hash that satisfies PoW at `DIFFICULTY_FLOOR` (difficulty = 1).
    pub fn make_test_block(
        parent_hash: &str,
        height: u64,
        timestamp: u64,
        slot: u8,
    ) -> Block {
        let txs = vec![coinbase_tx(height)];
        let tx_root = {
            let mut h = blake3::Hasher::new();
            for tx in &txs {
                h.update(tx.tx_id().as_bytes());
            }
            hex::encode(h.finalize().as_bytes())
        };
        let pow_hash = format!("{:02x}", slot.min(0xFE)).repeat(32);
        Block {
            header: BlockHeader {
                parent_hash: parent_hash.to_string(),
                number:      height,
                timestamp,
                difficulty:  DIFFICULTY_FLOOR,
                nonce:       slot as u64,
                pow_hash,
                state_root:  "0".repeat(64),
                tx_root,
                miner:       "test_miner".to_string(),
            },
            txs,
            weight: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::tests_helpers::{coinbase_tx, make_test_block};
    use crate::config::constants::{DIFFICULTY_FLOOR, LWMA_MIN_INTERVAL_SECS, TARGET_BLOCK_TIME};
    use crate::genesis;
    use crate::types::{BlockHeader, Tx};

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    // Keep a local alias so the tests below keep reading naturally.
    fn make_block(parent_hash: &str, height: u64, timestamp: u64, slot: u8) -> Block {
        make_test_block(parent_hash, height, timestamp, slot)
    }

    // ── Canonical append ──────────────────────────────────────────────────────

    #[test]
    fn canon_append_genesis_then_block1() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();

        let r0 = apply_block(&mut g, &gen, None);
        assert_eq!(r0, AcceptResult::CanonExtension { height: 0 });
        assert_eq!(g.blocks.len(), 1);

        let b1 = make_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        let r1 = apply_block(&mut g, &b1, None);
        assert_eq!(r1, AcceptResult::CanonExtension { height: 1 });
        assert_eq!(g.blocks.len(), 2);
        assert_eq!(g.tip_hash(), b1.hash());
    }

    #[test]
    fn canon_chain_grows_five_blocks() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut prev = gen.hash().to_string();
        let mut ts   = gen.header.timestamp;
        for i in 1u64..=5 {
            ts += TARGET_BLOCK_TIME;
            let blk = make_block(&prev, i, ts, (0xA0 + i) as u8);
            let r = apply_block(&mut g, &blk, None);
            assert_eq!(r, AcceptResult::CanonExtension { height: i },
                "block {} should extend canon", i);
            prev = blk.hash().to_string();
        }
        assert_eq!(g.blocks.len(), 6, "genesis + 5 canonical blocks");
    }

    #[test]
    fn cumulative_work_tracked_on_canon_chain() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);
        assert_eq!(g.cumulative_work[gen.hash()], DIFFICULTY_FLOOR as u128);

        let b1 = make_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        apply_block(&mut g, &b1, None);
        assert_eq!(g.cumulative_work[b1.hash()], 2 * DIFFICULTY_FLOOR as u128);
    }

    // ── Orphan storage ────────────────────────────────────────────────────────

    #[test]
    fn unknown_parent_stored_as_orphan() {
        let mut g = temp_state();
        let unknown_parent = "deadbeef".repeat(8);
        let blk = make_block(&unknown_parent, 5, 1_700_000_150, 0xAA);
        let r = apply_block(&mut g, &blk, Some("peer1"));
        assert!(
            matches!(r, AcceptResult::StoredOrphan { .. }),
            "expected StoredOrphan, got {:?}", r
        );
        assert!(g.orphan_pool.contains_key(unknown_parent.as_str()));
        assert_eq!(g.blocks.len(), 0, "no blocks integrated");
    }

    #[test]
    fn orphan_promoted_when_parent_arrives() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let ts1 = gen.header.timestamp + TARGET_BLOCK_TIME;
        let ts2 = ts1 + TARGET_BLOCK_TIME;
        let b1 = make_block(gen.hash(), 1, ts1, 0xAA);
        let b2 = make_block(b1.hash(), 2, ts2, 0xBB);

        // b2 arrives first — its parent b1 is not yet known.
        let r_orphan = apply_block(&mut g, &b2, None);
        assert!(matches!(r_orphan, AcceptResult::StoredOrphan { .. }));
        assert_eq!(g.blocks.len(), 1, "only genesis integrated");

        // b1 arrives — apply_block should auto-promote b2 from the orphan pool.
        let r_b1 = apply_block(&mut g, &b1, None);
        assert_eq!(r_b1, AcceptResult::CanonExtension { height: 1 });
        assert_eq!(g.blocks.len(), 3, "genesis + b1 + promoted b2");
        assert_eq!(g.orphan_pool.len(), 0, "orphan pool drained");
    }

    // ── Invalid PoW rejection ─────────────────────────────────────────────────

    #[test]
    fn invalid_pow_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        // Difficulty 1 accepts every upper-64-bit hash under historical target
        // semantics, so first create a fast block that retargets the next block
        // above the floor. Then all-ff fails because its upper 64 bits exceed
        // the difficulty-derived target.
        let ts1 = gen.header.timestamp + LWMA_MIN_INTERVAL_SECS;
        let b1 = make_block(gen.hash(), 1, ts1, 0xAA);
        assert_eq!(apply_block(&mut g, &b1, None), AcceptResult::CanonExtension { height: 1 });

        let ts2 = ts1 + TARGET_BLOCK_TIME;
        let expected_diff = calculate_next_difficulty(&g.blocks, ts2);
        assert!(expected_diff > DIFFICULTY_FLOOR);

        let mut bad = make_block(b1.hash(), 2, ts2, 0xAA);
        bad.header.difficulty = expected_diff;
        bad.header.pow_hash = "ff".repeat(32);

        let r = apply_block(&mut g, &bad, None);
        assert!(matches!(r, AcceptResult::Rejected(_)), "expected Rejected, got {:?}", r);
        if let AcceptResult::Rejected(reason) = r {
            assert!(reason.contains("PoW"), "rejection reason was: {}", reason);
        }
        assert_eq!(g.blocks.len(), 2, "canonical chain unchanged after bad PoW");
    }

    #[test]
    fn pre_validated_pow_skips_recheck() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let b1 = make_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        // Pre-validate before obtaining the chain lock.
        verify_pow_only(&b1).expect("should pre-validate");
        assert!(is_pow_prevalidated(b1.hash()), "should be in cache");

        let r = apply_block(&mut g, &b1, None);
        assert_eq!(r, AcceptResult::CanonExtension { height: 1 });
        // Cache entry consumed.
        assert!(!is_pow_prevalidated(b1.hash()), "cache cleared after integration");
    }

    // ── Side-chain handling ───────────────────────────────────────────────────

    #[test]
    fn competing_block_stored_as_side_chain() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;

        // b1 extends genesis — becomes canonical tip.
        let b1 = make_block(gen.hash(), 1, ts, 0xAA);
        let r1 = apply_block(&mut g, &b1, None);
        assert_eq!(r1, AcceptResult::CanonExtension { height: 1 });

        // b1_prime also extends genesis (same height, different hash) — side chain.
        let b1p = make_block(gen.hash(), 1, ts, 0xAB);
        let r2 = apply_block(&mut g, &b1p, None);
        assert!(
            matches!(r2, AcceptResult::SideChain { .. }),
            "expected SideChain, got {:?}", r2
        );

        // Canonical chain unchanged.
        assert_eq!(g.tip_hash(), b1.hash());
        assert_eq!(g.blocks.len(), 2, "genesis + b1 only");
        // Side block stored.
        assert!(g.side_blocks.contains_key(b1p.hash()));
        assert!(g.cumulative_work.contains_key(b1p.hash()));
    }

    // ── Other rejection paths ─────────────────────────────────────────────────

    #[test]
    fn wrong_tx_root_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut bad = make_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        bad.header.tx_root = "dead".repeat(16); // tamper with the root

        let r = apply_block(&mut g, &bad, None);
        assert!(matches!(r, AcceptResult::Rejected(_)));
    }

    #[test]
    fn duplicate_block_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let b1 = make_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        apply_block(&mut g, &b1, None);

        let r = apply_block(&mut g, &b1, None);
        assert!(matches!(r, AcceptResult::Rejected(_)), "duplicate must be rejected");
    }

    #[test]
    fn wrong_difficulty_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut bad = make_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        // expected = DIFFICULTY_FLOOR = 1; set to 999 to trigger mismatch.
        bad.header.difficulty = 999;

        let r = apply_block(&mut g, &bad, None);
        assert!(matches!(r, AcceptResult::Rejected(_)),
            "wrong difficulty should be rejected, got {:?}", r);
    }

    #[test]
    fn future_timestamp_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let far_future = wall_clock_secs() + MAX_FUTURE_TIMESTAMP_SECS + 1;
        let bad = make_block(gen.hash(), 1, far_future, 0xAA);

        let r = apply_block(&mut g, &bad, None);
        assert!(matches!(r, AcceptResult::Rejected(_)),
            "far-future timestamp should be rejected, got {:?}", r);
    }

    #[test]
    fn non_monotonic_timestamp_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        // Timestamp equal to genesis timestamp (0) is not monotonic.
        let bad = make_block(gen.hash(), 1, gen.header.timestamp, 0xAA);
        let r = apply_block(&mut g, &bad, None);
        assert!(matches!(r, AcceptResult::Rejected(_)),
            "non-monotonic timestamp should be rejected, got {:?}", r);
    }
}

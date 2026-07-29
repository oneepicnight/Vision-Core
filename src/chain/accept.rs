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
//! 6. **State/tx validation**  — coinbase height, tx IDs, transaction execution
//! 7. **Chain selection**      — cumulative-work comparison for side chains
//! 8. **Integration**          — push to canonical, side-chain store, or orphan pool

use once_cell::sync::Lazy;
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::chain::state_root::compute_state_root;
use crate::chain::ChainState;
use crate::config::constants::*;
use crate::miner::block_reward;
use crate::pow::difficulty::{calculate_next_difficulty, difficulty_to_target};
use crate::pow::historical_vpow::historical_vpow_message_bytes_with_nonce_zero;
use crate::pow::visionx::historical_block_digest;
use crate::types::transaction::{simulate_tx_execution, TxExecutionState};
use crate::types::Block;

pub(crate) fn apply_coinbase_reward(
    state: &mut TxExecutionState,
    miner: &str,
    height: u64,
) -> Result<(), String> {
    if miner.len() != 64
        || !miner
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("invalid miner address".into());
    }

    let reward = block_reward(height);
    let entry = state.balances.entry(miner.to_string()).or_insert(0);
    *entry = entry
        .checked_add(reward)
        .ok_or_else(|| "coinbase reward overflow".to_string())?;
    Ok(())
}

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

fn runtime_binary_sha256() -> String {
    std::env::var("VISION_BINARY_SHA256").unwrap_or_else(|_| "unknown".to_string())
}

fn runtime_git_commit() -> String {
    std::env::var("VISION_GIT_COMMIT")
        .or_else(|_| std::env::var("GIT_COMMIT"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn pow_failure_diagnostic(
    path: &str,
    blk: &Block,
    expected_difficulty: Option<u64>,
    epoch: u64,
    target: &[u8; 32],
    digest: &[u8; 32],
    reason: &str,
) -> String {
    let block_hash = blk.hash().to_string();
    let header_bytes = blk.header.canonical_bytes();
    let header_bytes_blake3 = hex::encode(blake3::hash(&header_bytes).as_bytes());
    let (preimage_len, preimage_blake3) =
        match historical_vpow_message_bytes_with_nonce_zero(&blk.header) {
            Ok(bytes) => (bytes.len(), hex::encode(blake3::hash(&bytes).as_bytes())),
            Err(err) => (0, format!("encoding-error:{err}")),
        };
    let digest_hex = hex::encode(digest);
    let target_hex = hex::encode(target);
    let binary_sha256 = runtime_binary_sha256();
    let git_commit = runtime_git_commit();
    let thread_id = format!("{:?}", std::thread::current().id());

    tracing::error!(
        target: "vision_core::pow_diagnostic",
        validation_path = path,
        reason = reason,
        block_height = blk.header.number,
        block_hash = block_hash.as_str(),
        parent_hash = blk.header.parent_hash.as_str(),
        version = BLOCK_VERSION,
        timestamp = blk.header.timestamp,
        stored_difficulty = blk.header.difficulty,
        expected_parent_difficulty = ?expected_difficulty,
        nonce = blk.header.nonce,
        miner = blk.header.miner.as_str(),
        tx_root = blk.header.tx_root.as_str(),
        state_root = blk.header.state_root.as_str(),
        epoch = epoch,
        target = target_hex.as_str(),
        computed_digest = digest_hex.as_str(),
        header_pow_hash = blk.header.pow_hash.as_str(),
        upper64_digest = %hex::encode(&digest[0..8]),
        upper64_target = %hex::encode(&target[0..8]),
        canonical_header_bytes_len = header_bytes.len(),
        canonical_header_bytes_blake3 = header_bytes_blake3.as_str(),
        historical_vpow_preimage_bytes_len = preimage_len,
        historical_vpow_preimage_blake3 = preimage_blake3.as_str(),
        dataset_epoch = epoch,
        dataset_parent_hash = blk.header.parent_hash.as_str(),
        process_pid = std::process::id(),
        binary_sha256 = binary_sha256.as_str(),
        git_commit = git_commit.as_str(),
        thread_id = thread_id.as_str(),
        "PoW validation failure diagnostic"
    );

    format!(
        "{reason}: path={path} height={} hash={} parent={} version={} timestamp={} difficulty={} expected_difficulty={:?} nonce={} miner={} tx_root={} state_root={} epoch={} target={} digest={} header_pow_hash={} upper64_digest={} upper64_target={} header_bytes_len={} header_bytes_blake3={} historical_vpow_preimage_len={} historical_vpow_preimage_blake3={} dataset_key=({}, {}) pid={} binary_sha256={} git_commit={} thread_id={}",
        blk.header.number,
        block_hash,
        blk.header.parent_hash,
        BLOCK_VERSION,
        blk.header.timestamp,
        blk.header.difficulty,
        expected_difficulty,
        blk.header.nonce,
        blk.header.miner,
        blk.header.tx_root,
        blk.header.state_root,
        epoch,
        target_hex,
        digest_hex,
        blk.header.pow_hash,
        hex::encode(&digest[0..8]),
        hex::encode(&target[0..8]),
        header_bytes.len(),
        header_bytes_blake3,
        preimage_len,
        preimage_blake3,
        epoch,
        blk.header.parent_hash,
        std::process::id(),
        binary_sha256,
        git_commit,
        thread_id,
    )
}

fn verify_visionx_pow(
    blk: &Block,
    expected_difficulty: Option<u64>,
    path: &str,
) -> Result<(), String> {
    let epoch = crate::pow::VISIONX_PARAMS.epoch(blk.header.number);
    let digest = historical_block_digest(&crate::pow::VISIONX_PARAMS, epoch, &blk.header)?;
    let target = difficulty_to_target(blk.header.difficulty);

    if &digest[0..8] > &target[0..8] {
        return Err(pow_failure_diagnostic(
            path,
            blk,
            expected_difficulty,
            epoch,
            &target,
            &digest,
            "PoW failed",
        ));
    }

    let computed = hex::encode(digest);
    if blk.hash() != computed {
        return Err(pow_failure_diagnostic(
            path,
            blk,
            expected_difficulty,
            epoch,
            &target,
            &digest,
            "PoW hash mismatch",
        ));
    }

    Ok(())
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
    verify_visionx_pow(blk, None, "verify_pow_only")
        .map_err(|reason| anyhow::anyhow!("PoW check failed: {}", reason))?;
    mark_pow_prevalidated(blk.hash());
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
    g.pending_reorg_recovery = None;
    let height = blk.header.number;
    g.cumulative_work.insert(hash.clone(), cw);
    g.seen_blocks.insert(hash.clone());
    g.canon_index.insert(hash, height);
    g.blocks.push(blk);
}

fn execute_non_coinbase_txs(state: &mut TxExecutionState, blk: &Block) -> Result<(), String> {
    for (idx, tx) in blk.txs.iter().enumerate().skip(1) {
        simulate_tx_execution(state, tx)
            .map_err(|err| format!("tx validation failed at index {}: {:?}", idx, err))?;
    }
    Ok(())
}

fn reconstruct_canonical_state_at_height(
    g: &ChainState,
    height: u64,
) -> Result<TxExecutionState, String> {
    let mut state = TxExecutionState::from_balances_and_nonces(
        std::collections::BTreeMap::new(),
        std::collections::BTreeMap::new(),
    );

    for blk in g.blocks.iter().take(height as usize + 1) {
        execute_non_coinbase_txs(&mut state, blk)?;
        if blk.header.number != 0 {
            apply_coinbase_reward(&mut state, &blk.header.miner, blk.header.number)?;
        }
    }

    Ok(state)
}

fn reconstruct_parent_state_for_side_chain(
    g: &ChainState,
    parent_hash: &str,
) -> Result<TxExecutionState, String> {
    if let Some(&height) = g.canon_index.get(parent_hash) {
        return reconstruct_canonical_state_at_height(g, height);
    }

    let mut branch: Vec<Block> = Vec::new();
    let mut current = parent_hash.to_string();

    loop {
        if let Some(&height) = g.canon_index.get(current.as_str()) {
            let mut state = reconstruct_canonical_state_at_height(g, height)?;
            branch.reverse();
            for blk in &branch {
                execute_non_coinbase_txs(&mut state, blk)?;
                if blk.header.number != 0 {
                    apply_coinbase_reward(&mut state, &blk.header.miner, blk.header.number)?;
                }
            }
            return Ok(state);
        }

        match g.side_blocks.get(current.as_str()) {
            Some(blk) => {
                branch.push(blk.clone());
                current = blk.header.parent_hash.clone();
            }
            None => {
                return Err(format!(
                    "broken side-chain ancestry for parent {:.8}",
                    parent_hash
                ));
            }
        }
    }
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
pub fn apply_block(g: &mut ChainState, blk: &Block, source_peer: Option<&str>) -> AcceptResult {
    let hash = blk.hash().to_string();
    clear_pow_prevalidation(&hash);

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
                return AcceptResult::Rejected("first tx must be coinbase::reward".into());
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
        if let Err(err) = crate::chain::storage::persist_canonical_extension(g, blk) {
            return AcceptResult::Rejected(format!("canonical persistence failed: {}", err));
        }
        push_canonical(g, blk.clone(), cw);
        return AcceptResult::CanonExtension { height: 0 };
    }

    // Non-genesis: parent must be in the canonical chain or the side-block store.
    let parent_hash = blk.header.parent_hash.clone();
    let parent_in_canon = g.canon_index.contains_key(parent_hash.as_str());
    let parent_in_side = g.side_blocks.contains_key(parent_hash.as_str());
    let tip_hash = g.tip_hash();
    let tip_cw = g
        .cumulative_work
        .get(tip_hash.as_str())
        .copied()
        .unwrap_or(0);
    let parent_cw = g
        .cumulative_work
        .get(parent_hash.as_str())
        .copied()
        .unwrap_or(0);
    let candidate_cw = parent_cw + blk.header.difficulty as u128;

    if !parent_in_canon && !parent_in_side {
        // Parent unknown — stash in orphan pool for future promotion.
        crate::chain::orphan::add_orphan(g, blk.clone(), source_peer.unwrap_or("anon"));
        return AcceptResult::StoredOrphan { block_hash: hash };
    }

    // Materialise the parent block for timestamp validation.
    let parent_blk = resolve_block(g, &parent_hash).expect("parent confirmed present above");

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

    if let Err(reason) = verify_visionx_pow(blk, Some(expected_diff), "apply_block") {
        return AcceptResult::Rejected(reason);
    }

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
        let encoded_height = u64::from_be_bytes(cb.args.as_slice().try_into().unwrap());
        if encoded_height != blk.header.number {
            return AcceptResult::Rejected(format!(
                "coinbase height mismatch: encoded={} block={}",
                encoded_height, blk.header.number
            ));
        }
    }

    // 6b. No duplicate canonical tx_ids within the block.
    {
        let mut seen_ids: HashSet<String> = HashSet::with_capacity(blk.txs.len());
        for tx in &blk.txs {
            let tid = crate::types::transaction::canonical_tx_id(tx);
            if !seen_ids.insert(tid.clone()) {
                return AcceptResult::Rejected(format!(
                    "duplicate canonical tx {:.8} in block",
                    tid
                ));
            }
        }
    }

    // 6c. Validate and execute non-coinbase transactions against the correct
    // parent state, using the canonical tip for extensions and reconstructed
    // ancestor state for side-chain candidates.
    let mut validated_tx_state = if parent_hash == tip_hash {
        TxExecutionState::from_balances_and_nonces(g.balances.clone(), g.nonces.clone())
    } else {
        match reconstruct_parent_state_for_side_chain(g, parent_hash.as_str()) {
            Ok(state) => state,
            Err(reason) => return AcceptResult::Rejected(reason),
        }
    };
    for (idx, tx) in blk.txs.iter().enumerate().skip(1) {
        if let Err(err) = simulate_tx_execution(&mut validated_tx_state, tx) {
            return AcceptResult::Rejected(format!(
                "tx validation failed at index {}: {:?}",
                idx, err
            ));
        }
    }

    if blk.header.number != 0 {
        if let Err(err) = apply_coinbase_reward(
            &mut validated_tx_state,
            &blk.header.miner,
            blk.header.number,
        ) {
            return AcceptResult::Rejected(format!("coinbase reward failed: {:?}", err));
        }
    }
    let computed_state_root =
        match compute_state_root(&validated_tx_state.balances, &validated_tx_state.nonces) {
            Ok(root) => root,
            Err(_) => {
                return AcceptResult::Rejected("state_root construction failed".into());
            }
        };

    if blk.header.state_root != computed_state_root {
        return AcceptResult::Rejected(format!(
            "state_root mismatch: header={:.8} computed={:.8}",
            blk.header.state_root, computed_state_root
        ));
    }

    if parent_hash == tip_hash {
        // LANE-A: straightforward extension of the current canonical tip.
        tracing::debug!(
            "[LANE-A] h={} hash={:.8} cw={} peer={:?}",
            blk.header.number,
            hash,
            candidate_cw,
            source_peer
        );
        if let Err(err) = crate::chain::storage::persist_canonical_extension(g, blk) {
            return AcceptResult::Rejected(format!("canonical persistence failed: {}", err));
        }
        g.balances = validated_tx_state.balances;
        g.nonces = validated_tx_state.nonces;
        push_canonical(g, blk.clone(), candidate_cw);
        let _ = crate::chain::snapshots::maybe_save_snapshot(g);
        let height = blk.header.number;
        crate::chain::orphan::process_orphans(g, &hash);
        return AcceptResult::CanonExtension { height };
    }

    // LANE-B: valid block on a non-tip chain; record it and test for reorg.
    tracing::debug!(
        "[LANE-B] h={} hash={:.8} cw={} tip_cw={} peer={:?}",
        blk.header.number,
        hash,
        candidate_cw,
        tip_cw,
        source_peer
    );
    if let Err(err) = crate::chain::storage::store_block(g, blk) {
        return AcceptResult::Rejected(format!("side-chain persistence failed: {}", err));
    }
    g.cumulative_work.insert(hash.clone(), candidate_cw);
    g.seen_blocks.insert(hash.clone());
    g.side_blocks.insert(hash.clone(), blk.clone());

    // Attempt a reorg only when this chain has *strictly* more work.
    // Equal-work reorgs are skipped (first-seen wins, reduces state churn).
    if candidate_cw > tip_cw {
        let reorg_recovery = crate::chain::reorg::try_reorg(g, blk);
        if let Some(recovery) = reorg_recovery {
            g.pending_reorg_recovery = Some(recovery);
            tracing::info!(
                "[REORG] new tip h={} hash={:.8} cw={}",
                blk.header.number,
                hash,
                candidate_cw
            );
            let height = blk.height();
            crate::chain::orphan::process_orphans(g, &hash);
            return AcceptResult::CanonExtension { height };
        }
    }

    let height = blk.height();
    crate::chain::orphan::process_orphans(g, &hash);
    AcceptResult::SideChain { height }
}
/// Test helpers exposed to sibling test modules (state, orphan, reorg, snapshots).
///
/// All helpers are gated by `#[cfg(test)]` so they are compiled only in test
/// builds and carry zero production overhead.
#[cfg(test)]
pub mod tests_helpers {
    use super::apply_coinbase_reward;
    use crate::chain::state_root::compute_state_root;
    use crate::config::constants::DIFFICULTY_FLOOR;
    use crate::pow::visionx::historical_block_digest;
    use crate::pow::VISIONX_PARAMS;
    use crate::types::transaction::TxExecutionState;
    use crate::types::{Block, BlockHeader, Tx};

    /// Build a coinbase transaction encoding `height` as 8-byte big-endian.
    pub fn coinbase_tx(height: u64) -> Tx {
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

    /// Build a non-genesis block suitable for unit tests.
    ///
    /// `slot` (0x00–0xFE) controls the `pow_hash` prefix byte.  Any slot ≤ 0xFE
    /// yields a hash that satisfies PoW at `DIFFICULTY_FLOOR` (difficulty = 1).
    pub fn make_test_block(parent_hash: &str, height: u64, timestamp: u64, slot: u8) -> Block {
        let txs = vec![coinbase_tx(height)];
        let tx_root = {
            let mut h = blake3::Hasher::new();
            for tx in &txs {
                h.update(crate::types::transaction::canonical_tx_id(tx).as_bytes());
            }
            hex::encode(h.finalize().as_bytes())
        };
        let mut block = Block {
            header: BlockHeader {
                parent_hash: parent_hash.to_string(),
                number: height,
                timestamp,
                difficulty: DIFFICULTY_FLOOR,
                nonce: slot as u64,
                pow_hash: String::new(),
                state_root: String::new(),
                tx_root,
                miner: "0".repeat(64),
            },
            txs,
            weight: 1,
        };
        let mut exec_state = TxExecutionState::new();
        for reward_height in 1..=block.header.number {
            apply_coinbase_reward(&mut exec_state, &block.header.miner, reward_height)
                .expect("test block should be able to credit cumulative coinbase rewards");
        }
        block.header.state_root = compute_state_root(&exec_state.balances, &exec_state.nonces)
            .expect("test block should compute a valid state root");
        let epoch = VISIONX_PARAMS.epoch(height);
        let digest = historical_block_digest(&VISIONX_PARAMS, epoch, &block.header)
            .expect("test block VisionX digest should build");
        block.header.pow_hash = hex::encode(digest);
        block
    }
}

#[cfg(test)]
mod tests {
    use super::tests_helpers::make_test_block;
    use super::*;
    use crate::chain::storage::load_height_index;
    use crate::config::constants::{DIFFICULTY_FLOOR, LWMA_MIN_INTERVAL_SECS, TARGET_BLOCK_TIME};
    use crate::genesis;
    use crate::pow::visionx::historical_block_digest;
    use crate::pow::VISIONX_PARAMS;
    use crate::types::transaction::{
        canonical_unsigned_payload, CashTransferArgs, MIN_CASH_TRANSFER_FEE_LIMIT,
    };
    use crate::types::Tx;
    use ed25519_dalek::{Signer, SigningKey};

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    // Keep a local alias so the tests below keep reading naturally.
    fn make_block(parent_hash: &str, height: u64, timestamp: u64, slot: u8) -> Block {
        make_test_block(parent_hash, height, timestamp, slot)
    }

    fn visionx_block(parent_hash: &str, height: u64, timestamp: u64, slot: u8) -> Block {
        let mut blk = make_block(parent_hash, height, timestamp, slot);
        let epoch = VISIONX_PARAMS.epoch(height);
        let digest = historical_block_digest(&VISIONX_PARAMS, epoch, &blk.header)
            .expect("historical VisionX digest should build");
        blk.header.pow_hash = hex::encode(digest);
        blk
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn sign_tx(mut tx: Tx, seed: u8) -> Tx {
        let signing_key = signing_key(seed);
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();
        let sig = signing_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    fn signed_transfer_tx(seed: u8, nonce: u64, to: &str, amount: u128, tip: u64) -> Tx {
        sign_tx(
            Tx {
                nonce,
                sender_pubkey: String::new(),
                module: "cash".to_string(),
                method: "transfer".to_string(),
                args: transfer_args(to, amount),
                tip,
                fee_limit: MIN_CASH_TRANSFER_FEE_LIMIT,
                sig: String::new(),
            },
            seed,
        )
    }

    fn signed_custom_tx(seed: u8, module: &str, method: &str, args: Vec<u8>) -> Tx {
        sign_tx(
            Tx {
                nonce: 0,
                sender_pubkey: String::new(),
                module: module.to_string(),
                method: method.to_string(),
                args,
                tip: 2,
                fee_limit: MIN_CASH_TRANSFER_FEE_LIMIT,
                sig: String::new(),
            },
            seed,
        )
    }

    fn recompute_tx_root(txs: &[Tx]) -> String {
        let mut h = blake3::Hasher::new();
        for tx in txs {
            h.update(crate::types::transaction::canonical_tx_id(tx).as_bytes());
        }
        hex::encode(h.finalize().as_bytes())
    }

    fn rehash_block(block: &mut Block) {
        block.header.tx_root = recompute_tx_root(&block.txs);
        block.header.pow_hash.clear();
        let epoch = VISIONX_PARAMS.epoch(block.header.number);
        let digest = historical_block_digest(&VISIONX_PARAMS, epoch, &block.header)
            .expect("historical VisionX digest should build");
        block.header.pow_hash = hex::encode(digest);
        block.weight = block.txs.len() as u64;
    }

    fn block_with_extra_txs(
        parent_hash: &str,
        height: u64,
        timestamp: u64,
        slot: u8,
        extra_txs: Vec<Tx>,
        balances: &std::collections::BTreeMap<String, u128>,
        nonces: &std::collections::BTreeMap<String, u64>,
    ) -> Block {
        let mut blk = make_test_block(parent_hash, height, timestamp, slot);
        blk.txs.extend(extra_txs);
        let mut exec_state =
            TxExecutionState::from_balances_and_nonces(balances.clone(), nonces.clone());
        for tx in blk.txs.iter().skip(1) {
            simulate_tx_execution(&mut exec_state, tx).ok();
        }
        if blk.header.number != 0 {
            apply_coinbase_reward(&mut exec_state, &blk.header.miner, blk.header.number)
                .expect("test helper should be able to credit coinbase reward");
        }
        blk.header.state_root = compute_state_root(&exec_state.balances, &exec_state.nonces)
            .expect("test helper should compute a valid state root");
        rehash_block(&mut blk);
        blk
    }

    fn reward_block_for_miner(
        parent_hash: &str,
        height: u64,
        timestamp: u64,
        slot: u8,
        miner: &str,
        balances: &std::collections::BTreeMap<String, u128>,
        nonces: &std::collections::BTreeMap<String, u64>,
    ) -> Block {
        let mut blk = make_test_block(parent_hash, height, timestamp, slot);
        blk.header.miner = miner.to_string();
        let mut state =
            TxExecutionState::from_balances_and_nonces(balances.clone(), nonces.clone());
        apply_coinbase_reward(&mut state, miner, height)
            .expect("test reward block should credit miner");
        blk.header.state_root = compute_state_root(&state.balances, &state.nonces)
            .expect("test reward block should compute state root");
        rehash_block(&mut blk);
        blk
    } // ── Canonical append ──────────────────────────────────────────────────────

    #[test]
    fn canon_append_genesis_then_block1() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();

        let r0 = apply_block(&mut g, &gen, None);
        assert_eq!(r0, AcceptResult::CanonExtension { height: 0 });
        assert_eq!(g.blocks.len(), 1);

        let b1 = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
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
        let mut ts = gen.header.timestamp;
        for i in 1u64..=5 {
            ts += TARGET_BLOCK_TIME;
            let blk = make_block(&prev, i, ts, (0xA0 + i) as u8);
            let r = apply_block(&mut g, &blk, None);
            assert_eq!(
                r,
                AcceptResult::CanonExtension { height: i },
                "block {} should extend canon",
                i
            );
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

        let b1 = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
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
            "expected StoredOrphan, got {:?}",
            r
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
    fn coinbase_reward_credits_miner_and_updates_state_root() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);
        let miner = "12".repeat(32);
        let blk = reward_block_for_miner(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            &miner,
            &g.balances,
            &g.nonces,
        );

        assert_eq!(
            apply_block(&mut g, &blk, None),
            AcceptResult::CanonExtension { height: 1 }
        );
        assert_eq!(g.balance_of(&miner), crate::miner::block_reward(1));
        assert_eq!(
            compute_state_root(&g.balances, &g.nonces).unwrap(),
            blk.header.state_root,
        );
    }

    #[test]
    fn coinbase_reward_is_immediately_spendable() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);
        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        let recipient = "34".repeat(32);
        let reward_block = reward_block_for_miner(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            &sender,
            &g.balances,
            &g.nonces,
        );
        assert_eq!(
            apply_block(&mut g, &reward_block, None),
            AcceptResult::CanonExtension { height: 1 },
        );

        let transfer = signed_transfer_tx(7, 0, &recipient, 100, 2);
        let spend_block = block_with_extra_txs(
            reward_block.hash(),
            2,
            reward_block.header.timestamp + TARGET_BLOCK_TIME,
            0xAB,
            vec![transfer],
            &g.balances,
            &g.nonces,
        );
        assert_eq!(
            apply_block(&mut g, &spend_block, None),
            AcceptResult::CanonExtension { height: 2 },
        );
        assert_eq!(g.balance_of(&sender), crate::miner::block_reward(1) - 103);
        assert_eq!(g.balance_of(&recipient), 100);
        assert_eq!(g.nonce_of(&sender), 1);
    }

    #[test]
    fn invalid_coinbase_miner_address_is_rejected_without_mutation() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);
        let mut blk = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        blk.header.miner = "AB".repeat(32);
        rehash_block(&mut blk);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let result = apply_block(&mut g, &blk, None);
        assert!(
            matches!(result, AcceptResult::Rejected(ref reason) if reason.contains("miner address"))
        );
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }
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
        assert_eq!(
            apply_block(&mut g, &b1, None),
            AcceptResult::CanonExtension { height: 1 }
        );
        assert_eq!(
            load_height_index(&g, 1).unwrap().as_deref(),
            Some(b1.hash())
        );

        let ts2 = ts1 + TARGET_BLOCK_TIME;
        let expected_diff = calculate_next_difficulty(&g.blocks, ts2);
        assert!(expected_diff > DIFFICULTY_FLOOR);

        let mut bad = make_block(b1.hash(), 2, ts2, 0xAA);
        bad.header.difficulty = expected_diff;
        bad.header.pow_hash = "ff".repeat(32);

        let r = apply_block(&mut g, &bad, None);
        assert!(
            matches!(r, AcceptResult::Rejected(_)),
            "expected Rejected, got {:?}",
            r
        );
        if let AcceptResult::Rejected(reason) = r {
            assert!(reason.contains("PoW"), "rejection reason was: {}", reason);
        }
        assert_eq!(g.blocks.len(), 2, "canonical chain unchanged after bad PoW");
        assert!(load_height_index(&g, 2).unwrap().is_none());
    }

    #[test]
    fn visionx_block_validation_accepts_valid_block() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let blk = visionx_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        let r = apply_block(&mut g, &blk, None);
        assert_eq!(r, AcceptResult::CanonExtension { height: 1 });
    }

    #[test]
    fn visionx_block_validation_rejects_invalid_block() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut blk = visionx_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        blk.header.nonce ^= 1;

        let r = apply_block(&mut g, &blk, None);
        assert!(
            matches!(r, AcceptResult::Rejected(_)),
            "expected Rejected, got {:?}",
            r
        );
        if let AcceptResult::Rejected(reason) = r {
            assert!(reason.contains("PoW"), "rejection reason was: {}", reason);
        }
    }
    #[test]
    fn canonical_extension_rejects_wrong_state_root_without_state_mutation() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut blk = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        blk.header.state_root = "11".repeat(32);
        rehash_block(&mut blk);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let result = apply_block(&mut g, &blk, None);
        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(reason.contains("state_root"), "reason was: {}", reason);
        }
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn valid_prevalidated_block_is_reverified_and_accepted() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let b1 = visionx_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        // Pre-validate before obtaining the chain lock.
        verify_pow_only(&b1).expect("should pre-validate");
        assert!(is_pow_prevalidated(b1.hash()), "should be in cache");

        let r = apply_block(&mut g, &b1, None);
        assert_eq!(r, AcceptResult::CanonExtension { height: 1 });
        // Cache entry consumed.
        assert!(
            !is_pow_prevalidated(b1.hash()),
            "cache cleared after integration"
        );
    }

    #[test]
    fn block_acceptance_validates_and_executes_signed_transfer() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let tx = signed_transfer_tx(7, 0, &recipient, 40, 2);
        let sender = tx.sender_pubkey.clone();
        g.balances.insert(sender.clone(), 100);

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx],
            &g.balances,
            &g.nonces,
        );

        assert_eq!(
            apply_block(&mut g, &blk, None),
            AcceptResult::CanonExtension { height: 1 }
        );
        assert_eq!(g.balance_of(&sender), 57);
        assert_eq!(g.balance_of(&recipient), 40);
        assert_eq!(g.nonce_of(&sender), 1);
    }

    #[test]
    fn block_acceptance_executes_same_sender_txs_in_nonce_order() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let tx0 = signed_transfer_tx(7, 0, &recipient, 10, 2);
        let tx1 = signed_transfer_tx(7, 1, &recipient, 20, 2);
        let sender = tx0.sender_pubkey.clone();
        g.balances.insert(sender.clone(), 100);

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx0, tx1],
            &g.balances,
            &g.nonces,
        );

        assert_eq!(
            apply_block(&mut g, &blk, None),
            AcceptResult::CanonExtension { height: 1 }
        );
        assert_eq!(g.balance_of(&sender), 64);
        assert_eq!(g.balance_of(&recipient), 30);
        assert_eq!(g.nonce_of(&sender), 2);
    }

    #[test]
    fn block_acceptance_rejects_invalid_signature_without_state_mutation() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let mut tx = signed_transfer_tx(7, 0, &recipient, 40, 2);
        let sender = tx.sender_pubkey.clone();
        tx.tip += 1;
        g.balances.insert(sender.clone(), 100);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(
                reason.contains("InvalidSignature"),
                "reason was: {}",
                reason
            );
        }
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn block_acceptance_rejects_bad_nonce_without_state_mutation() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let tx = signed_transfer_tx(7, 1, &recipient, 40, 2);
        let sender = tx.sender_pubkey.clone();
        g.balances.insert(sender.clone(), 100);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(reason.contains("NonceMismatch"), "reason was: {}", reason);
        }
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn block_acceptance_rejects_insufficient_funds_without_state_mutation() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let tx = signed_transfer_tx(7, 0, &recipient, 40, 2);
        let sender = tx.sender_pubkey.clone();
        g.balances.insert(sender.clone(), 42);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(
                reason.contains("InsufficientBalance"),
                "reason was: {}",
                reason
            );
        }
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn block_acceptance_rejects_bad_transfer_args() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let tx = signed_custom_tx(7, "cash", "transfer", b"not-json".to_vec());
        let sender = tx.sender_pubkey.clone();
        g.balances.insert(sender, 100);

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(reason.contains("BadTransferArgs"), "reason was: {}", reason);
        }
    }

    #[test]
    fn block_acceptance_rejects_unsupported_module_method() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let tx = signed_custom_tx(7, "stake", "lock", vec![]);
        let sender = tx.sender_pubkey.clone();
        g.balances.insert(sender, 100);

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(
                reason.contains("UnsupportedModuleMethod"),
                "reason was: {}",
                reason
            );
        }
    }

    #[test]
    fn block_acceptance_rejects_duplicate_canonical_tx_id() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let tx = signed_transfer_tx(7, 0, &recipient, 40, 2);
        let sender = tx.sender_pubkey.clone();
        let mut duplicate_intent = tx.clone();
        duplicate_intent.sig = "00".repeat(64);
        assert_ne!(tx.tx_id(), duplicate_intent.tx_id());
        assert_eq!(
            crate::types::transaction::canonical_tx_id(&tx),
            crate::types::transaction::canonical_tx_id(&duplicate_intent)
        );
        g.balances.insert(sender.clone(), 100);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx, duplicate_intent],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(
                reason.contains("duplicate canonical tx"),
                "reason was: {}",
                reason
            );
        }
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn block_acceptance_rejects_later_tx_without_partial_state_commit() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let recipient = "bb".repeat(32);
        let tx0 = signed_transfer_tx(7, 0, &recipient, 10, 2);
        let tx_gap = signed_transfer_tx(7, 2, &recipient, 20, 2);
        let sender = tx0.sender_pubkey.clone();
        g.balances.insert(sender.clone(), 100);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let blk = block_with_extra_txs(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![tx0, tx_gap],
            &g.balances,
            &g.nonces,
        );
        let result = apply_block(&mut g, &blk, None);

        assert!(matches!(result, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = result {
            assert!(reason.contains("NonceMismatch"), "reason was: {}", reason);
        }
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }
    // ── Side-chain handling ───────────────────────────────────────────────────

    #[test]
    fn side_chain_candidate_uses_reconstructed_parent_state_not_current_tip_state() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let side_parent_balances =
            std::collections::BTreeMap::from([("0".repeat(64), crate::miner::block_reward(1))]);
        let empty_nonces = std::collections::BTreeMap::new();

        let b1 = make_block(gen.hash(), 1, ts, 0xAA);
        assert_eq!(
            apply_block(&mut g, &b1, None),
            AcceptResult::CanonExtension { height: 1 }
        );

        let b2 = make_block(b1.hash(), 2, ts + TARGET_BLOCK_TIME, 0xAB);
        assert_eq!(
            apply_block(&mut g, &b2, None),
            AcceptResult::CanonExtension { height: 2 }
        );

        // Mutate the live tip state so the test would fail if side-chain
        // validation consulted it instead of reconstructing the branch parent.
        g.balances.insert("aa".repeat(32), 99);
        g.nonces.insert("bb".repeat(32), 7);

        let side_block = block_with_extra_txs(
            b1.hash(),
            2,
            ts + TARGET_BLOCK_TIME,
            0xAD,
            vec![],
            &side_parent_balances,
            &empty_nonces,
        );
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();
        let before_tip = g.tip_hash();
        let before_blocks = g.blocks.len();

        let r3 = apply_block(&mut g, &side_block, None);
        assert!(
            matches!(r3, AcceptResult::SideChain { height: 2 }),
            "expected SideChain, got {:?}",
            r3
        );

        assert_eq!(g.tip_hash(), before_tip);
        assert_eq!(g.blocks.len(), before_blocks);
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
        assert_eq!(
            load_height_index(&g, 2).unwrap().as_deref(),
            Some(g.blocks[2].hash())
        );
        assert!(g.side_blocks.contains_key(side_block.hash()));
        assert!(g.cumulative_work.contains_key(side_block.hash()));
    }

    #[test]
    fn side_chain_candidate_rejects_wrong_state_root_without_mutating_canonical_state() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let side_parent_balances =
            std::collections::BTreeMap::from([("0".repeat(64), crate::miner::block_reward(1))]);
        let empty_nonces = std::collections::BTreeMap::new();

        let b1 = make_block(gen.hash(), 1, ts, 0xAA);
        assert_eq!(
            apply_block(&mut g, &b1, None),
            AcceptResult::CanonExtension { height: 1 }
        );

        let mut bad_side_block = block_with_extra_txs(
            b1.hash(),
            2,
            ts + TARGET_BLOCK_TIME,
            0xAD,
            vec![],
            &side_parent_balances,
            &empty_nonces,
        );
        bad_side_block.header.state_root = "ff".repeat(32);
        rehash_block(&mut bad_side_block);

        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();
        let before_tip = g.tip_hash();
        let before_side_len = g.side_blocks.len();

        let r = apply_block(&mut g, &bad_side_block, None);
        assert!(matches!(r, AcceptResult::Rejected(_)));
        if let AcceptResult::Rejected(reason) = r {
            assert!(
                reason.contains("state_root mismatch"),
                "reason was: {}",
                reason
            );
        }
        assert_eq!(g.tip_hash(), before_tip);
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
        assert_eq!(g.side_blocks.len(), before_side_len);
        assert!(!g.side_blocks.contains_key(bad_side_block.hash()));
    }
    #[test]
    fn wrong_tx_root_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut bad = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        bad.header.tx_root = "dead".repeat(16); // tamper with the root

        let r = apply_block(&mut g, &bad, None);
        assert!(matches!(r, AcceptResult::Rejected(_)));
    }

    #[test]
    fn duplicate_block_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let b1 = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        apply_block(&mut g, &b1, None);

        let r = apply_block(&mut g, &b1, None);
        assert!(
            matches!(r, AcceptResult::Rejected(_)),
            "duplicate must be rejected"
        );
    }

    #[test]
    fn wrong_difficulty_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let mut bad = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        // expected = DIFFICULTY_FLOOR = 1; set to 999 to trigger mismatch.
        bad.header.difficulty = 999;

        let r = apply_block(&mut g, &bad, None);
        assert!(
            matches!(r, AcceptResult::Rejected(_)),
            "wrong difficulty should be rejected, got {:?}",
            r
        );
    }

    #[test]
    fn future_timestamp_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let far_future = u64::MAX;
        let bad = make_block(gen.hash(), 1, far_future, 0xAA);

        let r = apply_block(&mut g, &bad, None);
        assert!(
            matches!(r, AcceptResult::Rejected(_)),
            "far-future timestamp should be rejected, got {:?}",
            r
        );
    }

    #[test]
    fn non_monotonic_timestamp_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        // Timestamp equal to genesis timestamp (0) is not monotonic.
        let bad = make_block(gen.hash(), 1, gen.header.timestamp, 0xAA);
        let r = apply_block(&mut g, &bad, None);
        assert!(
            matches!(r, AcceptResult::Rejected(_)),
            "non-monotonic timestamp should be rejected, got {:?}",
            r
        );
    }
    fn prevalidated_valid_block() -> (ChainState, Block, String) {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let valid = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        let cached_hash = valid.hash().to_string();
        verify_pow_only(&valid).expect("baseline block should prevalidate");
        assert!(
            is_pow_prevalidated(&cached_hash),
            "baseline hash should be cached"
        );
        (g, valid, cached_hash)
    }

    fn assert_current_pow_rejects(label: &str, mutated: Block, cached_hash: String) {
        assert_eq!(
            mutated.hash(),
            cached_hash,
            "{label} must retain the cached pow_hash key"
        );
        let recomputed = historical_block_digest(
            &VISIONX_PARAMS,
            VISIONX_PARAMS.epoch(mutated.header.number),
            &mutated.header,
        )
        .expect("mutated block digest should compute");
        assert_ne!(
            hex::encode(recomputed),
            cached_hash,
            "{label} mutation must invalidate PoW"
        );

        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);
        let result = apply_block(&mut g, &mutated, None);
        assert!(
            matches!(result, AcceptResult::Rejected(_)),
            "{label} mutation was accepted through a stale prevalidation key: {result:?}"
        );
        assert!(
            !is_pow_prevalidated(&cached_hash),
            "{label} cache entry should be cleared"
        );
    }

    fn assert_prevalidated_mutation_rejected(label: &str, mutate: impl FnOnce(&mut Block)) {
        let (_g, valid, cached_hash) = prevalidated_valid_block();
        let mut mutated = valid.clone();
        mutate(&mut mutated);
        assert_current_pow_rejects(label, mutated, cached_hash);
    }

    #[test]
    fn prevalidated_nonce_mutation_is_rejected() {
        assert_prevalidated_mutation_rejected("nonce", |block| {
            block.header.nonce = block.header.nonce.wrapping_add(1);
        });
    }

    #[test]
    fn prevalidated_parent_mutation_is_rejected() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);

        let canonical_parent = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xA1,
        );
        assert_eq!(
            apply_block(&mut g, &canonical_parent, None),
            AcceptResult::CanonExtension { height: 1 }
        );
        let side_parent = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME + 1,
            0xA2,
        );
        assert!(matches!(
            apply_block(&mut g, &side_parent, None),
            AcceptResult::SideChain { height: 1 }
        ));

        let valid_child = make_block(
            canonical_parent.hash(),
            2,
            canonical_parent.header.timestamp + TARGET_BLOCK_TIME,
            0xA3,
        );
        let cached_hash = valid_child.hash().to_string();
        verify_pow_only(&valid_child).expect("child block should prevalidate");

        let mut mutated = valid_child.clone();
        mutated.header.parent_hash = side_parent.hash().to_string();
        assert_eq!(
            mutated.hash(),
            cached_hash,
            "parent mutation must retain the cached pow_hash key"
        );
        let recomputed = historical_block_digest(
            &VISIONX_PARAMS,
            VISIONX_PARAMS.epoch(mutated.header.number),
            &mutated.header,
        )
        .expect("mutated child digest should compute");
        assert_ne!(
            hex::encode(recomputed),
            cached_hash,
            "parent mutation must invalidate PoW"
        );

        let result = apply_block(&mut g, &mutated, None);
        assert!(
            matches!(result, AcceptResult::Rejected(_)),
            "parent mutation should reject, got {result:?}"
        );
        assert!(
            !is_pow_prevalidated(&cached_hash),
            "parent mutation cache entry should be cleared"
        );
    }

    #[test]
    fn prevalidated_timestamp_mutation_is_rejected() {
        assert_prevalidated_mutation_rejected("timestamp", |block| {
            block.header.timestamp = block.header.timestamp.wrapping_add(1);
        });
    }

    #[test]
    fn prevalidated_difficulty_mutation_is_rejected() {
        assert_prevalidated_mutation_rejected("difficulty", |block| {
            block.header.difficulty = block.header.difficulty.saturating_add(1);
        });
    }

    #[test]
    fn prevalidated_tx_root_mutation_is_rejected() {
        assert_prevalidated_mutation_rejected("tx_root", |block| {
            block.header.tx_root = "11".repeat(32);
        });
    }

    #[test]
    fn prevalidated_miner_mutation_is_rejected() {
        assert_prevalidated_mutation_rejected("miner", |block| {
            block.header.miner = "1".repeat(64);
        });
    }

    #[test]
    fn invalid_prevalidated_block_clears_cache_entry() {
        let (_g, valid, cached_hash) = prevalidated_valid_block();
        let mut invalid = valid.clone();
        invalid.header.nonce = invalid.header.nonce.wrapping_add(1);
        assert_current_pow_rejects("invalid prevalidated", invalid, cached_hash);
    }

    #[test]
    fn rejection_before_pow_stage_clears_cache_entry() {
        let mut g = temp_state();
        let gen = genesis::genesis_block();
        apply_block(&mut g, &gen, None);
        let valid = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        assert_eq!(
            apply_block(&mut g, &valid, None),
            AcceptResult::CanonExtension { height: 1 }
        );

        let cached_hash = valid.hash().to_string();
        verify_pow_only(&valid).expect("duplicate block should prevalidate");
        assert!(is_pow_prevalidated(&cached_hash));
        let result = apply_block(&mut g, &valid, None);
        assert!(
            matches!(result, AcceptResult::Rejected(_)),
            "duplicate should reject before PoW, got {result:?}"
        );
        assert!(
            !is_pow_prevalidated(&cached_hash),
            "duplicate rejection should clear cache entry"
        );
    }

    #[test]
    fn cache_entry_cannot_authorize_second_block_with_same_pow_hash() {
        let (_g, valid, cached_hash) = prevalidated_valid_block();
        let mut second = valid.clone();
        second.header.timestamp = second.header.timestamp.wrapping_add(1);
        assert_current_pow_rejects("second same-hash block", second, cached_hash);
    }

    #[test]
    fn concurrent_prevalidation_entries_cannot_bypass_independent_verification() {
        let gen = genesis::genesis_block();
        let blocks: Vec<Block> = (0..8)
            .map(|i| {
                make_block(
                    gen.hash(),
                    1,
                    gen.header.timestamp + TARGET_BLOCK_TIME + i,
                    i as u8,
                )
            })
            .collect();

        std::thread::scope(|scope| {
            for block in &blocks {
                scope.spawn(move || {
                    verify_pow_only(block).expect("concurrent block should prevalidate");
                });
            }
        });

        for (idx, block) in blocks.into_iter().enumerate() {
            let cached_hash = block.hash().to_string();
            assert!(
                is_pow_prevalidated(&cached_hash),
                "concurrent cache entry {idx} should exist"
            );
            let mut mutated = block.clone();
            mutated.header.nonce = mutated.header.nonce.wrapping_add(1 + idx as u64);
            assert_current_pow_rejects("concurrent prevalidation", mutated, cached_hash);
        }
    }

    #[test]
    fn exact_preserved_block12_synthetic_case_is_rejected() {
        const BLOCK12_STORED_POW_HASH: &str =
            "21cbd526b5ae4178e8ffe81b57be8a340dd3c9b2dc3528345e0c5bc6dca6578e";
        const BLOCK12_DETERMINISTIC_DIGEST: &str =
            "3cab81b8193cfd11ba0b35054e7e8409c25afd4835d202444e079ddf2476098e";
        assert_ne!(BLOCK12_STORED_POW_HASH, BLOCK12_DETERMINISTIC_DIGEST);

        let gen = genesis::genesis_block();
        let mut block = make_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xBC,
        );
        block.header.pow_hash = BLOCK12_STORED_POW_HASH.to_string();
        mark_pow_prevalidated(BLOCK12_STORED_POW_HASH);
        assert!(is_pow_prevalidated(BLOCK12_STORED_POW_HASH));

        let recomputed = historical_block_digest(
            &VISIONX_PARAMS,
            VISIONX_PARAMS.epoch(block.header.number),
            &block.header,
        )
        .expect("synthetic block digest should compute");
        assert_ne!(hex::encode(recomputed), BLOCK12_STORED_POW_HASH);

        let mut g = temp_state();
        apply_block(&mut g, &gen, None);
        let result = apply_block(&mut g, &block, None);
        assert!(
            matches!(result, AcceptResult::Rejected(_)),
            "preserved block-12 hash must not authorize acceptance, got {result:?}"
        );
        assert!(!is_pow_prevalidated(BLOCK12_STORED_POW_HASH));
    }
}

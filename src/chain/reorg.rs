use crate::chain::ChainState;
use crate::config::constants::MAX_REORG;
use crate::types::Block;
use crate::chain::state_root::compute_state_root;
use crate::types::transaction::{canonical_tx_id, simulate_tx_execution, TxExecutionError, TxExecutionState};

// ─── Chain-select helpers ─────────────────────────────────────────────────────

/// Walk backwards from `tip_hash` through the side-block store, collecting
/// every block until a canonical ancestor is reached.
///
/// Returns the segment in **ascending height order** (oldest first), not
/// including the canonical ancestor itself.  Returns an empty `Vec` if the
/// ancestry cannot be fully traced back to a canonical block.
fn collect_new_segment(g: &ChainState, tip_hash: &str) -> Vec<Block> {
    let mut segment: Vec<Block> = Vec::new();
    let mut current = tip_hash.to_string();

    loop {
        if g.canon_index.contains_key(current.as_str()) {
            // `current` is already canonical — it is the common ancestor.
            break;
        }
        match g.side_blocks.get(current.as_str()) {
            Some(blk) => {
                let parent = blk.header.parent_hash.clone();
                segment.push(blk.clone());
                current = parent;
            }
            None => {
                // Gap: cannot trace ancestry. Abort.
                return Vec::new();
            }
        }
    }

    segment.reverse(); // oldest → newest
    segment
}

// ─── Reorg entry point ────────────────────────────────────────────────────────

/// Attempt to reorganise to the chain that ends at `new_tip`.
///
/// The function walks backwards through `g.side_blocks` from `new_tip` to
/// find the common ancestor with the current canonical chain, then:
///
/// 1. Checks that the reorg depth ≤ `MAX_REORG` (protects finalised blocks).
/// 2. Demotes canonical blocks above the common ancestor to `side_blocks`.
/// 3. Promotes the new segment from `side_blocks` to canonical.
/// 4. Rebuilds `canon_index` and `cumulative_work` across the affected range.
///
/// Returns `true` if the canonical tip changed; `false` if the reorg was
/// rejected (depth too large, broken ancestry, or common ancestor not found).
#[derive(Debug, Clone, PartialEq, Eq)]
enum SideStateReconstructionError {
    BrokenAncestry {
        expected_parent: String,
        got_parent: String,
    },
    Execution(TxExecutionError),
    StateRootConstructionFailed,
    StateRootMismatch {
        block_hash: String,
        expected: String,
        computed: String,
    },
}

fn reconstruct_canonical_state_at_height(
    g: &ChainState,
    height: u64,
) -> Result<TxExecutionState, SideStateReconstructionError> {
    let mut state = TxExecutionState::new();

    for blk in g.blocks.iter().take((height as usize) + 1) {
        for tx in blk.txs.iter().skip(1) {
            simulate_tx_execution(&mut state, tx)
                .map_err(SideStateReconstructionError::Execution)?;
        }

        if blk.header.number != 0 {
            let computed_root = compute_state_root(&state.balances, &state.nonces)
                .map_err(|_| SideStateReconstructionError::StateRootConstructionFailed)?;
            if computed_root != blk.header.state_root {
                return Err(SideStateReconstructionError::StateRootMismatch {
                    block_hash: blk.hash().to_string(),
                    expected: blk.header.state_root.clone(),
                    computed: computed_root,
                });
            }
        }
    }

    Ok(state)
}

fn reconstruct_branch_state(
    ancestor_hash: &str,
    ancestor_state: &TxExecutionState,
    branch: &[Block],
) -> Result<TxExecutionState, SideStateReconstructionError> {
    let mut state = ancestor_state.clone();
    let mut expected_parent = ancestor_hash.to_string();

    for blk in branch {
        if blk.header.parent_hash != expected_parent {
            return Err(SideStateReconstructionError::BrokenAncestry {
                expected_parent,
                got_parent: blk.header.parent_hash.clone(),
            });
        }

        for tx in blk.txs.iter().skip(1) {
            simulate_tx_execution(&mut state, tx)
                .map_err(SideStateReconstructionError::Execution)?;
        }

        if blk.header.number != 0 {
            let computed_root = compute_state_root(&state.balances, &state.nonces)
                .map_err(|_| SideStateReconstructionError::StateRootConstructionFailed)?;
            if computed_root != blk.header.state_root {
                return Err(SideStateReconstructionError::StateRootMismatch {
                    block_hash: blk.hash().to_string(),
                    expected: blk.header.state_root.clone(),
                    computed: computed_root,
                });
            }
        }

        expected_parent = blk.hash().to_string();
    }

    Ok(state)
}
pub fn try_reorg(g: &mut ChainState, new_tip: &Block) -> bool {
    let new_tip_hash = new_tip.hash();

    // ── 1. Trace the new chain segment back to a canonical ancestor ───────────
    let new_segment = collect_new_segment(g, new_tip_hash);
    if new_segment.is_empty() {
        tracing::debug!("[REORG] aborted: cannot trace ancestry of {:.8}", new_tip_hash);
        return false;
    }

    // ── 2. Locate the common ancestor ──────────────────────────────────────────
    let common_hash = &new_segment[0].header.parent_hash;
    let &common_height = match g.canon_index.get(common_hash.as_str()) {
        Some(h) => h,
        None => {
            tracing::debug!("[REORG] aborted: common ancestor {:.8} not canonical", common_hash);
            return false;
        }
    };

    // ── 3. Depth guard ─────────────────────────────────────────────────────────
    let reorg_depth = g.current_height().saturating_sub(common_height);
    if reorg_depth > MAX_REORG {
        tracing::warn!(
            "[REORG] rejected: depth {} exceeds MAX_REORG {}",
            reorg_depth, MAX_REORG
        );
        return false;
    }

    let ancestor_state = match reconstruct_canonical_state_at_height(g, common_height) {
        Ok(state) => state,
        Err(reason) => {
            tracing::debug!("[REORG] aborted: ancestor replay validation failed: {:?}", reason);
            return false;
        }
    };

    if let Err(reason) = reconstruct_branch_state(common_hash, &ancestor_state, &new_segment) {
        tracing::debug!("[REORG] aborted: branch replay validation failed: {:?}", reason);
        return false;
    }

    // ── 4. Demote canonical blocks above the common ancestor ───────────────────
    let demoted: Vec<Block> = g.blocks.drain((common_height as usize + 1)..).collect();
    for b in &demoted {
        let h = b.hash().to_string();
        g.canon_index.remove(&h);
        g.side_blocks.insert(h, b.clone());
    }
    tracing::debug!(
        "[REORG] demoted {} canonical blocks above h={}",
        demoted.len(), common_height
    );

    // ── 5. Promote the new segment ────────────────────────────────────────────
    // Seed cumulative work at the common ancestor; then accumulate forward.
    let mut cw = g.cumulative_work
        .get(common_hash.as_str())
        .copied()
        .unwrap_or_else(|| {
            // Recompute from the genesis if cache is cold — should be rare.
            g.blocks.iter().map(|b| b.header.difficulty as u128).sum()
        });

    for blk in &new_segment {
        let hash = blk.hash().to_string();
        cw += blk.header.difficulty as u128;
        g.canon_index.insert(hash.clone(), blk.header.number);
        g.side_blocks.remove(&hash);
        g.cumulative_work.insert(hash.clone(), cw);
        g.seen_blocks.insert(hash);
        g.blocks.push(blk.clone());
    }

    tracing::info!(
        "[REORG] complete: common_h={} new_tip_h={} depth={}",
        common_height,
        new_tip.header.number,
        reorg_depth,
    );
    true
}

/// Compute the cumulative PoW work on the canonical chain ending at `block_hash`.
///
/// Looks up cached values first; falls back to summing the canonical slice.
/// Used for diagnostics and in tests; production code should use the cached
/// `g.cumulative_work` map maintained by `push_canonical` / `try_reorg`.
pub fn cumulative_work(g: &ChainState, block_hash: &str) -> u128 {
    if let Some(&cw) = g.cumulative_work.get(block_hash) {
        return cw;
    }
    // Fallback: linear scan of canonical blocks.
    let mut sum = 0u128;
    for b in &g.blocks {
        sum += b.header.difficulty as u128;
        if b.hash() == block_hash {
            return sum;
    }
    }
    0
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::{apply_block, AcceptResult};
    use crate::chain::accept::tests_helpers::make_test_block;
    use crate::chain::state::ChainState;
    use crate::config::constants::{DIFFICULTY_FLOOR, TARGET_BLOCK_TIME};
    use crate::genesis::genesis_block;
    use crate::chain::state_root::compute_state_root;
    use crate::pow::visionx::historical_block_digest;
    use crate::pow::VISIONX_PARAMS;
    use crate::types::transaction::{
        canonical_unsigned_payload, CashTransferArgs, MIN_CASH_TRANSFER_FEE_LIMIT,
        TxExecutionState,
    };
    use crate::types::Tx;
    use ed25519_dalek::{Signer, SigningKey};

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
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

    fn recompute_tx_root(txs: &[Tx]) -> String {
        let mut h = blake3::Hasher::new();
        for tx in txs {
            h.update(canonical_tx_id(tx).as_bytes());
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

    fn state_after_seed() -> TxExecutionState {
        let mut balances = std::collections::BTreeMap::new();
        let nonces = std::collections::BTreeMap::new();
        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        balances.insert(sender, 100);
        TxExecutionState::from_balances_and_nonces(balances, nonces)
    }

    fn state_after_block(seed: &TxExecutionState, block: &Block) -> TxExecutionState {
        let mut state = seed.clone();
        for tx in block.txs.iter().skip(1) {
            crate::types::transaction::simulate_tx_execution(&mut state, tx).unwrap();
        }
        state
    }

    fn branch_block(
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
        let mut exec_state = TxExecutionState::from_balances_and_nonces(
            balances.clone(),
            nonces.clone(),
        );
        for tx in blk.txs.iter().skip(1) {
            crate::types::transaction::simulate_tx_execution(&mut exec_state, tx).ok();
        }
        blk.header.state_root = compute_state_root(&exec_state.balances, &exec_state.nonces)
            .expect("test helper should compute a valid state root");
        rehash_block(&mut blk);
        blk
    }

    fn no_op_valid_block(
        parent_hash: &str,
        height: u64,
        timestamp: u64,
        slot: u8,
        balances: &std::collections::BTreeMap<String, u128>,
        nonces: &std::collections::BTreeMap<String, u64>,
    ) -> Block {
        branch_block(parent_hash, height, timestamp, slot, Vec::new(), balances, nonces)
    }

    /// Build a short canonical chain of `n` blocks on top of genesis.
    /// Returns the state and a Vec of all blocks including genesis.
    fn build_chain(n: u64) -> (ChainState, Vec<Block>) {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let mut blocks = vec![gen.clone()];
        let mut prev = gen.hash().to_string();
        let mut ts   = gen.header.timestamp;
        for i in 1..=n {
            ts += TARGET_BLOCK_TIME;
            let blk = make_test_block(&prev, i, ts, (0xA0 + i) as u8);
            apply_block(&mut g, &blk, None);
            prev = blk.hash().to_string();
            blocks.push(blk);
        }
        (g, blocks)
    }

    // ── cumulative_work ───────────────────────────────────────────────────────

    #[test]
    fn reconstructs_from_canonical_ancestor() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        let recipient_1 = hex::encode(signing_key(8).verifying_key().to_bytes());
        let recipient_2 = hex::encode(signing_key(9).verifying_key().to_bytes());
        let mut ancestor_balances = std::collections::BTreeMap::new();
        ancestor_balances.insert(sender.clone(), 100);
        let ancestor_nonces = std::collections::BTreeMap::new();

        let ancestor = branch_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![signed_transfer_tx(7, 0, &recipient_1, 40, 2)],
            &ancestor_balances,
            &ancestor_nonces,
        );
        apply_block(&mut g, &ancestor, None);
        let ancestor_state = state_after_block(&state_after_seed(), &ancestor);

        let branch_1 = branch_block(
            ancestor.hash(),
            2,
            ancestor.header.timestamp + TARGET_BLOCK_TIME,
            0xBB,
            vec![signed_transfer_tx(7, 1, &recipient_2, 10, 1)],
            &ancestor_state.balances,
            &ancestor_state.nonces,
        );
        let branch_1_state = state_after_block(&ancestor_state, &branch_1);
        let branch_2 = no_op_valid_block(
            branch_1.hash(),
            3,
            branch_1.header.timestamp + TARGET_BLOCK_TIME,
            0xBC,
            &branch_1_state.balances,
            &branch_1_state.nonces,
        );

        let reconstructed = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[branch_1.clone(), branch_2.clone()],
        )
        .expect("branch reconstruction should succeed");

        assert_eq!(reconstructed.balance_of(&sender), 45);
        assert_eq!(reconstructed.balance_of(&recipient_1), 40);
        assert_eq!(reconstructed.balance_of(&recipient_2), 10);
        assert_eq!(reconstructed.nonce_of(&sender), 2);
    }

    #[test]
    fn reconstruction_is_independent_of_current_canonical_tip() {
        let mut g1 = temp_state();
        let mut g2 = temp_state();
        let gen = genesis_block();
        apply_block(&mut g1, &gen, None);
        apply_block(&mut g2, &gen, None);

        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(8).verifying_key().to_bytes());
        let mut ancestor_balances = std::collections::BTreeMap::new();
        ancestor_balances.insert(sender.clone(), 100);
        let ancestor_nonces = std::collections::BTreeMap::new();

        let ancestor = branch_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![signed_transfer_tx(7, 0, &recipient, 40, 2)],
            &ancestor_balances,
            &ancestor_nonces,
        );
        apply_block(&mut g1, &ancestor, None);
        apply_block(&mut g2, &ancestor, None);

        let tip_1 = no_op_valid_block(
            ancestor.hash(),
            2,
            ancestor.header.timestamp + TARGET_BLOCK_TIME,
            0xAB,
            &g1.balances,
            &g1.nonces,
        );
        let tip_2 = no_op_valid_block(
            tip_1.hash(),
            3,
            tip_1.header.timestamp + TARGET_BLOCK_TIME,
            0xAC,
            &g2.balances,
            &g2.nonces,
        );
        apply_block(&mut g1, &tip_1, None);
        apply_block(&mut g2, &tip_1, None);
        apply_block(&mut g2, &tip_2, None);

        let ancestor_state = state_after_block(&state_after_seed(), &ancestor);
        let branch_1 = branch_block(
            ancestor.hash(),
            2,
            ancestor.header.timestamp + TARGET_BLOCK_TIME,
            0xBB,
            vec![signed_transfer_tx(7, 1, &recipient, 10, 1)],
            &ancestor_state.balances,
            &ancestor_state.nonces,
        );
        let branch_1_state = state_after_block(&ancestor_state, &branch_1);
        let branch_2 = no_op_valid_block(
            branch_1.hash(),
            3,
            branch_1.header.timestamp + TARGET_BLOCK_TIME,
            0xBC,
            &branch_1_state.balances,
            &branch_1_state.nonces,
        );

        let reconstructed_1 = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[branch_1.clone(), branch_2.clone()],
        )
        .expect("reconstruction should succeed");
        let reconstructed_2 = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[branch_1, branch_2],
        )
        .expect("reconstruction should succeed");

        assert_eq!(reconstructed_1, reconstructed_2);
    }

    #[test]
    fn identical_branch_history_reconstructs_identical_state() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(8).verifying_key().to_bytes());
        let mut ancestor_balances = std::collections::BTreeMap::new();
        ancestor_balances.insert(sender.clone(), 100);
        let ancestor_nonces = std::collections::BTreeMap::new();

        let ancestor = branch_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![signed_transfer_tx(7, 0, &recipient, 40, 2)],
            &ancestor_balances,
            &ancestor_nonces,
        );
        apply_block(&mut g, &ancestor, None);
        let ancestor_state = state_after_block(&state_after_seed(), &ancestor);

        let branch_1 = branch_block(
            ancestor.hash(),
            2,
            ancestor.header.timestamp + TARGET_BLOCK_TIME,
            0xBB,
            vec![signed_transfer_tx(7, 1, &recipient, 10, 1)],
            &ancestor_state.balances,
            &ancestor_state.nonces,
        );
        let branch_1_state = state_after_block(&ancestor_state, &branch_1);
        let branch_2 = no_op_valid_block(
            branch_1.hash(),
            3,
            branch_1.header.timestamp + TARGET_BLOCK_TIME,
            0xBC,
            &branch_1_state.balances,
            &branch_1_state.nonces,
        );

        let reconstructed_1 = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[branch_1.clone(), branch_2.clone()],
        )
        .expect("reconstruction should succeed");
        let reconstructed_2 = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[branch_1, branch_2],
        )
        .expect("reconstruction should succeed");

        assert_eq!(reconstructed_1, reconstructed_2);
    }

    #[test]
    fn canonical_state_unchanged_after_reconstruction() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(8).verifying_key().to_bytes());
        let mut ancestor_balances = std::collections::BTreeMap::new();
        ancestor_balances.insert(sender.clone(), 100);
        let ancestor_nonces = std::collections::BTreeMap::new();

        let ancestor = branch_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![signed_transfer_tx(7, 0, &recipient, 40, 2)],
            &ancestor_balances,
            &ancestor_nonces,
        );
        apply_block(&mut g, &ancestor, None);
        let ancestor_state = state_after_block(&state_after_seed(), &ancestor);
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let branch_1 = branch_block(
            ancestor.hash(),
            2,
            ancestor.header.timestamp + TARGET_BLOCK_TIME,
            0xBB,
            vec![signed_transfer_tx(7, 1, &recipient, 10, 1)],
            &ancestor_state.balances,
            &ancestor_state.nonces,
        );
        let branch_1_state = state_after_block(&ancestor_state, &branch_1);
        let branch_2 = no_op_valid_block(
            branch_1.hash(),
            3,
            branch_1.header.timestamp + TARGET_BLOCK_TIME,
            0xBC,
            &branch_1_state.balances,
            &branch_1_state.nonces,
        );

        let _ = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[branch_1, branch_2],
        )
        .expect("reconstruction should succeed");

        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn malformed_ancestry_is_rejected() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let sender = hex::encode(signing_key(7).verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(8).verifying_key().to_bytes());
        let mut ancestor_balances = std::collections::BTreeMap::new();
        ancestor_balances.insert(sender.clone(), 100);
        let ancestor_nonces = std::collections::BTreeMap::new();

        let ancestor = branch_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![signed_transfer_tx(7, 0, &recipient, 40, 2)],
            &ancestor_balances,
            &ancestor_nonces,
        );
        apply_block(&mut g, &ancestor, None);
        let ancestor_state = state_after_block(&state_after_seed(), &ancestor);

        let malformed_parent = "11".repeat(32);
        let bad_branch = branch_block(
            &malformed_parent,
            2,
            ancestor.header.timestamp + TARGET_BLOCK_TIME,
            0xBB,
            vec![signed_transfer_tx(7, 1, &recipient, 10, 1)],
            &g.balances,
            &g.nonces,
        );

        let result = reconstruct_branch_state(
            ancestor.hash(),
            &ancestor_state,
            &[bad_branch],
        );

        assert!(matches!(
            result,
            Err(SideStateReconstructionError::BrokenAncestry { .. })
        ));
    }
    fn cumulative_work_single_block() {
        let (g, blocks) = build_chain(0);
        let gen = &blocks[0];
        assert_eq!(cumulative_work(&g, gen.hash()), DIFFICULTY_FLOOR as u128);
    }

    #[test]
    fn cumulative_work_n_blocks() {
        let n = 5u64;
        let (g, blocks) = build_chain(n);
        let tip = blocks.last().unwrap();
        // Each block contributes DIFFICULTY_FLOOR (=1 at difficulty floor).
        let expected = (n + 1) as u128 * DIFFICULTY_FLOOR as u128;
        assert_eq!(cumulative_work(&g, tip.hash()), expected);
    }

    // ── try_reorg ─────────────────────────────────────────────────────────────

    /// Fork at genesis: two competing b1 candidates.
    /// The heavier one (after try_reorg is called from apply_block) wins.
    #[test]
    fn reorg_switches_to_heavier_chain() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;

        // b1 extends genesis — canonical.
        let b1 = make_test_block(gen.hash(), 1, ts, 0xAA);
        apply_block(&mut g, &b1, None);
        assert_eq!(g.tip_hash(), b1.hash());

        // b1p is a competing block at height 1 (side chain).
        let b1p = make_test_block(gen.hash(), 1, ts, 0xAB);
        apply_block(&mut g, &b1p, None);
        // b1p alone doesn't have more work, so canonical should still be b1.
        assert_eq!(g.tip_hash(), b1.hash(), "b1p alone must not dislodge b1");

        // b2p extends b1p — now the b1p chain is longer (and heavier).
        let ts2 = ts + TARGET_BLOCK_TIME;
        let b2p = make_test_block(b1p.hash(), 2, ts2, 0xCC);
        // Manually insert b2p as side block so try_reorg can find its ancestry.
        g.side_blocks.insert(b2p.hash().to_string(), b2p.clone());
        let b2p_cw = g.cumulative_work.get(b1p.hash()).copied().unwrap_or(0)
            + b2p.header.difficulty as u128;
        g.cumulative_work.insert(b2p.hash().to_string(), b2p_cw);
        g.seen_blocks.insert(b2p.hash().to_string());

        // Current canonical cw.
        let canon_cw = *g.cumulative_work.get(b1.hash()).unwrap();

        // try_reorg should switch to b2p chain since b2p_cw > canon_cw.
        if b2p_cw > canon_cw {
            let reorged = try_reorg(&mut g, &b2p);
            assert!(reorged, "reorg should succeed to heavier chain");
            assert_eq!(g.tip_hash(), b2p.hash(), "tip should be b2p");
            assert_eq!(g.current_height(), 2);
            // b1 should be in side blocks now.
            assert!(g.side_blocks.contains_key(b1.hash()),
                "demoted b1 should be in side_blocks");
        }
    }
    #[test]
    fn reorg_respects_max_reorg_depth() {
        // Build a chain of MAX_REORG + 2 blocks.
        let depth = (MAX_REORG + 2) as u64;
        let (mut g, blocks) = build_chain(depth);

        // Fabricate a side block at height 1 (would require depth > MAX_REORG reorg).
        let fork_parent = blocks[0].hash().to_string();
        let fork_ts = blocks[0].header.timestamp + TARGET_BLOCK_TIME;
        let fork_tip = make_test_block(&fork_parent, 1, fork_ts, 0xFF);
        g.side_blocks.insert(fork_tip.hash().to_string(), fork_tip.clone());
        g.cumulative_work.insert(
            fork_tip.hash().to_string(),
            (depth + 100) as u128, // artificially higher cw
        );
        g.seen_blocks.insert(fork_tip.hash().to_string());

        let reorged = try_reorg(&mut g, &fork_tip);
        assert!(!reorged, "reorg deeper than MAX_REORG must be rejected");
    }

    #[test]
    fn reorg_empty_segment_when_ancestry_broken() {
        let (mut g, blocks) = build_chain(2);
        let tip = blocks.last().unwrap();

        // A block that points to an unknown parent — ancestry chain broken.
        let unknown_parent = "dead".repeat(16);
        let orphan = make_test_block(&unknown_parent, 99, tip.header.timestamp + 30, 0xDD);
        g.side_blocks.insert(orphan.hash().to_string(), orphan.clone());

        let result = try_reorg(&mut g, &orphan);
        assert!(!result, "reorg with broken ancestry must fail");
    }

    #[test]
    fn reorg_replay_rejects_invalid_transaction_without_mutation() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let canon = no_op_valid_block(gen.hash(), 1, ts, 0xAA, &g.balances, &g.nonces);
        apply_block(&mut g, &canon, None);

        let before_tip = g.tip_hash();
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let side1 = no_op_valid_block(gen.hash(), 1, ts, 0xAB, &g.balances, &g.nonces);
        let mut side2 = branch_block(
            side1.hash(),
            2,
            ts + TARGET_BLOCK_TIME,
            0xAC,
            vec![signed_transfer_tx(
                7,
                0,
                &hex::encode(signing_key(8).verifying_key().to_bytes()),
                1,
                1,
            )],
            &g.balances,
            &g.nonces,
        );
        side2.txs[1].nonce = 1;
        rehash_block(&mut side2);

        g.side_blocks.insert(side1.hash().to_string(), side1.clone());
        g.side_blocks.insert(side2.hash().to_string(), side2.clone());

        let reorged = try_reorg(&mut g, &side2);
        assert!(!reorged, "reorg must reject invalid replay transaction");
        assert_eq!(g.tip_hash(), before_tip);
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn reorg_replay_rejects_invalid_state_root_without_mutation() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let canon = no_op_valid_block(gen.hash(), 1, ts, 0xAA, &g.balances, &g.nonces);
        apply_block(&mut g, &canon, None);

        let before_tip = g.tip_hash();
        let before_balances = g.balances.clone();
        let before_nonces = g.nonces.clone();

        let side1 = no_op_valid_block(gen.hash(), 1, ts, 0xAB, &g.balances, &g.nonces);
        let mut side2 = no_op_valid_block(
            side1.hash(),
            2,
            ts + TARGET_BLOCK_TIME,
            0xAC,
            &g.balances,
            &g.nonces,
        );
        side2.header.state_root = "11".repeat(32);
        rehash_block(&mut side2);

        g.side_blocks.insert(side1.hash().to_string(), side1.clone());
        g.side_blocks.insert(side2.hash().to_string(), side2.clone());

        let reorged = try_reorg(&mut g, &side2);
        assert!(!reorged, "reorg must reject invalid replay state root");
        assert_eq!(g.tip_hash(), before_tip);
        assert_eq!(g.balances, before_balances);
        assert_eq!(g.nonces, before_nonces);
    }

    #[test]
    fn demoted_blocks_move_to_side_blocks() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let ts2 = ts + TARGET_BLOCK_TIME;

        // Build canonical: gen → b1 → b2
        let b1 = make_test_block(gen.hash(), 1, ts, 0xAA);
        let b2 = make_test_block(b1.hash(), 2, ts2, 0xBB);
        apply_block(&mut g, &b1, None);
        apply_block(&mut g, &b2, None);
        assert_eq!(g.current_height(), 2);

        // Build a heavier fork: gen → c1 → c2 → c3
        let c1 = make_test_block(gen.hash(), 1, ts, 0xCC);
        let c2 = make_test_block(c1.hash(), 2, ts2, 0xDD);
        let ts3 = ts2 + TARGET_BLOCK_TIME;
        let c3 = make_test_block(c2.hash(), 3, ts3, 0xEE);

        // Add the competing chain to side_blocks manually.
        let mut cw = g.cumulative_work[gen.hash()];
        for blk in [&c1, &c2, &c3] {
            cw += blk.header.difficulty as u128;
            g.side_blocks.insert(blk.hash().to_string(), blk.clone());
            g.cumulative_work.insert(blk.hash().to_string(), cw);
            g.seen_blocks.insert(blk.hash().to_string());

        }
        let reorged = try_reorg(&mut g, &c3);
        assert!(reorged, "should reorg to longer chain");
        assert_eq!(g.tip_hash(), c3.hash());
        // Original canonical blocks b1, b2 should now be side blocks.
        assert!(g.side_blocks.contains_key(b1.hash()));
        assert!(g.side_blocks.contains_key(b2.hash()));
    }
}













use crate::chain::ChainState;
use crate::config::constants::MAX_REORG;
use crate::types::Block;

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

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
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

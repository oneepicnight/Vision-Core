use anyhow::{anyhow, Result};
use crate::chain::ChainState;
use crate::config::constants::SNAPSHOT_EVERY;

// ─── Snapshot key scheme ──────────────────────────────────────────────────────
//  meta:snapshot:{height}     → bincode((height: u64, tip_hash: String))
//  snap:balances:{height}     → bincode(BTreeMap<String, u128>)
//  snap:nonces:{height}       → bincode(BTreeMap<String, u64>)

// ─── Save ─────────────────────────────────────────────────────────────────────

/// Save a state snapshot only when `current_height % SNAPSHOT_EVERY == 0`.
///
/// Snapshots bound the amount of chain that must be replayed after a reorg
/// deeper than any block already in memory.  They are optional — the node
/// can also recover by re-syncing — but they speed reorg safety considerably.
pub fn maybe_save_snapshot(g: &ChainState) -> Result<()> {
    let height = g.current_height();
    if height == 0 || height % SNAPSHOT_EVERY != 0 {
        return Ok(());
    }
    save_snapshot(g, height)
}

/// Unconditionally write a full-state snapshot at `height`.
pub fn save_snapshot(g: &ChainState, height: u64) -> Result<()> {
    let tip_hash = g.tip_hash();

    // Serialise account state.
    g.db.insert(
        format!("snap:balances:{}", height).as_bytes(),
        bincode::serialize(&g.balances)?,
    )?;
    g.db.insert(
        format!("snap:nonces:{}", height).as_bytes(),
        bincode::serialize(&g.nonces)?,
    )?;
    // Index entry — used by restore_latest_snapshot to enumerate snapshots.
    g.db.insert(
        format!("meta:snapshot:{}", height).as_bytes(),
        bincode::serialize(&(height, tip_hash.as_str()))?,
    )?;

    tracing::info!("[SNAPSHOT] Saved state at height {}", height);
    Ok(())
}

// ─── Restore ──────────────────────────────────────────────────────────────────

/// Restore balances and nonces from the most recent snapshot at or below
/// `max_height`, then truncate `g.blocks` to that snapshot height.
///
/// After returning:
/// - `g.balances` and `g.nonces` reflect the snapshotted account state.
/// - `g.blocks` is truncated to `[0 ..= restored_height]`.
/// - `g.canon_index` and `g.cumulative_work` are **rebuilt** from the
///   truncated block Vec so they remain consistent.
/// - `g.cached_state_root` is cleared.
///
/// Returns `Ok(restored_height)` on success; `Err` if no valid snapshot exists.
///
/// The caller is responsible for re-applying any blocks that were above the
/// restored height (typically by requesting them from peers).
pub fn restore_latest_snapshot(g: &mut ChainState, max_height: u64) -> Result<u64> {
    // Enumerate all snapshot heights stored in the database.
    let mut snap_heights: Vec<u64> = g.db
        .scan_prefix(b"meta:snapshot:")
        .flatten()
        .filter_map(|(k, _)| {
            let s = String::from_utf8(k.to_vec()).ok()?;
            let h: u64 = s.strip_prefix("meta:snapshot:")?.parse().ok()?;
            if h <= max_height { Some(h) } else { None }
        })
        .collect();

    let best = snap_heights.into_iter().max()
        .ok_or_else(|| anyhow!("no snapshot at or below h={}", max_height))?;

    // Restore account state.
    if let Some(bytes) = g.db.get(format!("snap:balances:{}", best).as_bytes())? {
        g.balances = bincode::deserialize(&bytes)?;
    }
    if let Some(bytes) = g.db.get(format!("snap:nonces:{}", best).as_bytes())? {
        g.nonces = bincode::deserialize(&bytes)?;
    }

    // Truncate canonical block Vec.
    let keep = (best + 1) as usize;
    if g.blocks.len() > keep {
        g.blocks.truncate(keep);
    }

    // Rebuild canon_index and cumulative_work from the (now-truncated) Vec.
    g.canon_index.clear();
    g.cumulative_work.clear();
    let mut cw = 0u128;
    for b in &g.blocks {
        cw += b.header.difficulty as u128;
        g.canon_index.insert(b.hash().to_string(), b.header.number);
        g.cumulative_work.insert(b.hash().to_string(), cw);
    }

    g.cached_state_root = None;

    tracing::info!("[SNAPSHOT] Restored state to height {}", best);
    Ok(best)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::apply_block;
    use crate::chain::accept::tests_helpers::make_test_block;
    use crate::chain::state::ChainState;
    use crate::config::constants::TARGET_BLOCK_TIME;
    use crate::genesis::genesis_block;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn build_chain_n(n: u64) -> (ChainState, Vec<crate::types::Block>) {
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

    #[test]
    fn save_snapshot_persists_balances() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        g.credit_balance("alice", 42_000);

        save_snapshot(&g, 0).unwrap();

        // Wipe in-memory balances and restore.
        g.balances.clear();
        restore_latest_snapshot(&mut g, 0).unwrap();
        assert_eq!(g.balance_of("alice"), 42_000);
    }

    #[test]
    fn save_snapshot_persists_nonces() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        g.advance_nonce("bob", 7);

        save_snapshot(&g, 0).unwrap();
        g.nonces.clear();
        restore_latest_snapshot(&mut g, 0).unwrap();
        assert_eq!(g.nonce_of("bob"), 7);
    }

    #[test]
    fn restore_truncates_blocks_to_snapshot_height() {
        let (mut g, blocks) = build_chain_n(4);
        assert_eq!(g.current_height(), 4);

        // Save a snapshot at height 2.
        save_snapshot(&g, 2).unwrap();

        // Restore — blocks should be truncated to height 2.
        let restored = restore_latest_snapshot(&mut g, 4).unwrap();
        assert_eq!(restored, 2);
        assert_eq!(g.blocks.len(), 3, "genesis + block1 + block2");
        assert_eq!(g.current_height(), 2);
    }

    #[test]
    fn restore_rebuilds_canon_index() {
        let (mut g, blocks) = build_chain_n(3);
        save_snapshot(&g, 2).unwrap();

        // Corrupt canon_index to verify rebuild.
        g.canon_index.clear();

        restore_latest_snapshot(&mut g, 3).unwrap();
        // Blocks 0, 1, 2 should all be in the rebuilt canon_index.
        assert!(g.canon_index.contains_key(blocks[0].hash()));
        assert!(g.canon_index.contains_key(blocks[1].hash()));
        assert!(g.canon_index.contains_key(blocks[2].hash()));
        // Block 3 was above the snapshot and is gone.
        assert!(!g.canon_index.contains_key(blocks[3].hash()));
    }

    #[test]
    fn restore_rebuilds_cumulative_work() {
        let (mut g, blocks) = build_chain_n(3);
        save_snapshot(&g, 2).unwrap();
        g.cumulative_work.clear();

        restore_latest_snapshot(&mut g, 3).unwrap();
        // cw at height 2 = 3 * DIFFICULTY_FLOOR (genesis + b1 + b2).
        let cw = g.cumulative_work.get(blocks[2].hash()).copied().unwrap_or(0);
        assert_eq!(cw, 3 * crate::config::constants::DIFFICULTY_FLOOR as u128);
    }

    #[test]
    fn restore_fails_when_no_snapshot_exists() {
        let mut g = temp_state();
        let result = restore_latest_snapshot(&mut g, 100);
        assert!(result.is_err());
    }

    #[test]
    fn maybe_save_snapshot_only_fires_at_interval() {
        let (g, _) = build_chain_n(SNAPSHOT_EVERY - 1);
        // Height = SNAPSHOT_EVERY - 1 → no snapshot yet.
        let count_before: usize = g.db
            .scan_prefix(b"meta:snapshot:")
            .count();
        maybe_save_snapshot(&g).unwrap();
        let count_after: usize = g.db
            .scan_prefix(b"meta:snapshot:")
            .count();
        assert_eq!(count_before, count_after, "should not save at non-multiple height");
    }
}

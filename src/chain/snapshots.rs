use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

use crate::chain::state_root::compute_state_root;
use crate::chain::storage::{load_block, load_height_index};
use crate::chain::ChainState;
use crate::config::constants::SNAPSHOT_EVERY;

// --- Snapshot key scheme ---
//  meta:snapshot:{height}          -> bincode((height: u64, tip_hash: String))
//  meta:snapshot_state_root:{height} -> bincode(String)
//  snap:balances:{height}          -> bincode(BTreeMap<String, u128>)
//  snap:nonces:{height}            -> bincode(BTreeMap<String, u64>)

// --- Save ---

/// Save a state snapshot only when `current_height % SNAPSHOT_EVERY == 0`.
///
/// Snapshots bound the amount of chain that must be replayed after a reorg
/// deeper than any block already in memory. They are optional - the node can
/// also recover by re-syncing - but they speed reorg safety considerably.
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
    let state_root = compute_state_root(&g.balances, &g.nonces).map_err(|e| {
        anyhow!(
            "snapshot state root computation failed at h={}: {:?}",
            height,
            e
        )
    })?;

    // Serialise account state.
    g.db.insert(
        format!("snap:balances:{}", height).as_bytes(),
        bincode::serialize(&g.balances)?,
    )?;
    g.db.insert(
        format!("snap:nonces:{}", height).as_bytes(),
        bincode::serialize(&g.nonces)?,
    )?;
    // Index entry used by restore_latest_snapshot to enumerate snapshots.
    g.db.insert(
        format!("meta:snapshot:{}", height).as_bytes(),
        bincode::serialize(&(height, tip_hash.as_str()))?,
    )?;
    g.db.insert(
        format!("meta:snapshot_state_root:{}", height).as_bytes(),
        bincode::serialize(&state_root)?,
    )?;

    tracing::info!("[SNAPSHOT] Saved state at height {}", height);
    Ok(())
}

// --- Restore ---

fn normalize_snapshot_state(
    balances: &mut BTreeMap<String, u128>,
    nonces: &mut BTreeMap<String, u64>,
) {
    balances.retain(|_, amount| *amount != 0);
    nonces.retain(|_, nonce| *nonce != 0);
}

/// Restore balances and nonces from the most recent snapshot at or below
/// `max_height`, then truncate `g.blocks` to that snapshot height.
///
/// After returning:
/// - `g.balances` and `g.nonces` reflect the snapshotted account state.
/// - `g.blocks` is truncated to `[0 ..= restored_height]`.
/// - `g.canon_index` and `g.cumulative_work` are rebuilt from the truncated
///   block Vec so they remain consistent.
/// - `g.cached_state_root` is repopulated with the restored height and root.
///
/// Returns `Ok(restored_height)` on success; `Err` if no valid snapshot exists.
///
/// The caller is responsible for re-applying any blocks that were above the
/// restored height (typically by requesting them from peers).
pub fn restore_latest_snapshot(g: &mut ChainState, max_height: u64) -> Result<u64> {
    // Enumerate all snapshot heights stored in the database. Newest snapshots
    // are preferred, but height alone is not trusted: the stored snapshot hash
    // and root must match the persisted canonical block at that height.
    let mut snap_heights: Vec<u64> =
        g.db.scan_prefix(b"meta:snapshot:")
            .flatten()
            .filter_map(|(k, _)| {
                let s = String::from_utf8(k.to_vec()).ok()?;
                let h: u64 = s.strip_prefix("meta:snapshot:")?.parse().ok()?;
                (h <= max_height).then_some(h)
            })
            .collect();
    snap_heights.sort_unstable_by(|a, b| b.cmp(a));

    let mut rejection_reasons = Vec::new();
    for height in snap_heights {
        match restore_snapshot_at(g, height) {
            Ok(()) => {
                tracing::info!("[SNAPSHOT] Restored state to height {}", height);
                return Ok(height);
            }
            Err(err) => {
                tracing::warn!(
                    "[SNAPSHOT] Rejecting snapshot at height {} during startup restore: {}",
                    height,
                    err
                );
                rejection_reasons.push(format!("h{}: {}", height, err));
            }
        }
    }

    if rejection_reasons.is_empty() {
        Err(anyhow!("no snapshot at or below h={}", max_height))
    } else {
        Err(anyhow!(
            "no valid canonical snapshot at or below h={}: {}",
            max_height,
            rejection_reasons.join("; ")
        ))
    }
}

fn restore_snapshot_at(g: &mut ChainState, height: u64) -> Result<()> {
    let metadata_bytes =
        g.db.get(format!("meta:snapshot:{}", height).as_bytes())?
            .ok_or_else(|| anyhow!("missing snapshot metadata"))?;
    let (metadata_height, snapshot_hash): (u64, String) = bincode::deserialize(&metadata_bytes)?;
    if metadata_height != height {
        return Err(anyhow!(
            "snapshot metadata height mismatch: key={} metadata={}",
            height,
            metadata_height
        ));
    }

    let canonical_hash = load_height_index(g, height)?
        .ok_or_else(|| anyhow!("missing canonical height index at h={}", height))?;
    if snapshot_hash != canonical_hash {
        return Err(anyhow!(
            "snapshot hash {} does not match canonical hash {} at h={}",
            snapshot_hash,
            canonical_hash,
            height
        ));
    }
    let canonical_block = load_block(g, &canonical_hash)?
        .ok_or_else(|| anyhow!("canonical block {} missing at h={}", canonical_hash, height))?;

    let mut balances: BTreeMap<String, u128> =
        g.db.get(format!("snap:balances:{}", height).as_bytes())?
            .map(|bytes| bincode::deserialize(&bytes))
            .transpose()?
            .unwrap_or_default();
    let mut nonces: BTreeMap<String, u64> =
        g.db.get(format!("snap:nonces:{}", height).as_bytes())?
            .map(|bytes| bincode::deserialize(&bytes))
            .transpose()?
            .unwrap_or_default();
    normalize_snapshot_state(&mut balances, &mut nonces);

    let computed_state_root = compute_state_root(&balances, &nonces).map_err(|e| {
        anyhow!(
            "restored snapshot state is invalid at h={}: {:?}",
            height,
            e
        )
    })?;

    if let Some(bytes) =
        g.db.get(format!("meta:snapshot_state_root:{}", height).as_bytes())?
    {
        let stored_state_root: String = bincode::deserialize(&bytes)?;
        if stored_state_root != computed_state_root {
            return Err(anyhow!(
                "snapshot state root mismatch at h={}: stored={} computed={}",
                height,
                stored_state_root,
                computed_state_root
            ));
        }
    }

    let is_genesis_dev_snapshot =
        height == 0 && canonical_block.header.state_root == "0".repeat(64);
    if canonical_block.header.state_root != computed_state_root && !is_genesis_dev_snapshot {
        return Err(anyhow!(
            "snapshot state root {} does not match canonical block root {} at h={}",
            computed_state_root,
            canonical_block.header.state_root,
            height
        ));
    }

    g.balances = balances;
    g.nonces = nonces;

    // Truncate canonical block Vec.
    let keep = (height + 1) as usize;
    if g.blocks.len() > keep {
        g.blocks.truncate(keep);
    }

    // Rebuild canon_index and cumulative_work from the truncated Vec.
    g.canon_index.clear();
    g.cumulative_work.clear();
    let mut cw = 0u128;
    for b in &g.blocks {
        cw += b.header.difficulty as u128;
        g.canon_index.insert(b.hash().to_string(), b.header.number);
        g.cumulative_work.insert(b.hash().to_string(), cw);
    }

    g.seen_blocks.clear();
    for b in &g.blocks {
        g.seen_blocks.insert(b.hash().to_string());
    }

    g.cached_state_root = Some((height, computed_state_root));
    Ok(())
}
// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::apply_block;
    use crate::chain::accept::tests_helpers::make_test_block;
    use crate::chain::state::ChainState;
    use crate::chain::state_root::compute_state_root;
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
        let mut ts = gen.header.timestamp;
        for i in 1..=n {
            ts += TARGET_BLOCK_TIME;
            let blk = make_test_block(&prev, i, ts, (0xA0 + i) as u8);
            apply_block(&mut g, &blk, None);
            prev = blk.hash().to_string();
            blocks.push(blk);
        }
        (g, blocks)
    }

    fn build_chain_n_with_snapshots(
        n: u64,
        snapshot_heights: &[u64],
    ) -> (ChainState, Vec<crate::types::Block>) {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let mut blocks = vec![gen.clone()];
        let mut prev = gen.hash().to_string();
        let mut ts = gen.header.timestamp;
        for i in 1..=n {
            ts += TARGET_BLOCK_TIME;
            let blk = make_test_block(&prev, i, ts, (0xA0 + i) as u8);
            apply_block(&mut g, &blk, None);
            prev = blk.hash().to_string();
            blocks.push(blk);
            if snapshot_heights.contains(&i) {
                save_snapshot(&g, i).unwrap();
            }
        }
        (g, blocks)
    }
    #[test]
    fn snapshot_restore_preserves_coinbase_reward_balance() {
        let (mut g, blocks) = build_chain_n(1);
        let miner = "0".repeat(64);
        let expected_reward = crate::miner::block_reward(1);
        let expected_root = blocks[1].header.state_root.clone();
        assert_eq!(g.balance_of(&miner), expected_reward);

        save_snapshot(&g, 1).unwrap();
        g.balances.clear();
        g.nonces.clear();
        g.cached_state_root = None;
        restore_latest_snapshot(&mut g, 1).unwrap();

        assert_eq!(g.balance_of(&miner), expected_reward);
        assert_eq!(g.cached_state_root, Some((1, expected_root)));
    }
    #[test]
    fn save_snapshot_persists_balances() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let alice = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        g.credit_balance(alice, 42_000);

        save_snapshot(&g, 0).unwrap();

        g.balances.clear();
        g.cached_state_root = None;
        restore_latest_snapshot(&mut g, 0).unwrap();
        assert_eq!(g.balance_of(alice), 42_000);
    }

    #[test]
    fn save_snapshot_persists_nonces() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let bob = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        g.advance_nonce(bob, 7);

        save_snapshot(&g, 0).unwrap();
        g.nonces.clear();
        g.cached_state_root = None;
        restore_latest_snapshot(&mut g, 0).unwrap();
        assert_eq!(g.nonce_of(bob), 7);
    }

    #[test]
    fn restore_recomputes_state_root_and_cache() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        g.credit_balance(key, 57);
        g.advance_nonce(key, 1);
        save_snapshot(&g, 0).unwrap();

        g.balances.clear();
        g.nonces.clear();
        g.cached_state_root = None;

        restore_latest_snapshot(&mut g, 0).unwrap();
        let expected = compute_state_root(&g.balances, &g.nonces).unwrap();
        assert_eq!(g.cached_state_root, Some((0, expected)));
    }

    #[test]
    fn restore_rejects_mismatched_snapshot_state_root_metadata() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        g.credit_balance(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1,
        );
        save_snapshot(&g, 0).unwrap();

        g.db.insert(
            b"meta:snapshot_state_root:0",
            bincode::serialize(&"00".repeat(32)).unwrap(),
        )
        .unwrap();

        g.balances.clear();
        g.nonces.clear();
        g.cached_state_root = None;

        assert!(restore_latest_snapshot(&mut g, 0).is_err());
    }

    #[test]
    fn restore_rejects_malformed_balance_keys() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        save_snapshot(&g, 0).unwrap();

        let mut balances = BTreeMap::new();
        balances.insert("not-hex".to_string(), 1);
        g.db.insert(b"snap:balances:0", bincode::serialize(&balances).unwrap())
            .unwrap();

        assert!(restore_latest_snapshot(&mut g, 0).is_err());
    }

    #[test]
    fn restore_rejects_mixed_case_nonces_keys() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        save_snapshot(&g, 0).unwrap();

        let mut nonces = BTreeMap::new();
        nonces.insert(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            1,
        );
        g.db.insert(b"snap:nonces:0", bincode::serialize(&nonces).unwrap())
            .unwrap();

        assert!(restore_latest_snapshot(&mut g, 0).is_err());
    }

    #[test]
    fn restore_normalizes_zero_entries() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let zero_balance =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let zero_nonce =
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        let active = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
        g.balances.insert(zero_balance.clone(), 0);
        g.nonces.insert(zero_nonce.clone(), 0);
        g.credit_balance(&active, 40);
        g.advance_nonce(&active, 1);
        save_snapshot(&g, 0).unwrap();

        g.balances.clear();
        g.nonces.clear();
        g.cached_state_root = None;
        restore_latest_snapshot(&mut g, 0).unwrap();

        assert!(!g.balances.contains_key(&zero_balance));
        assert!(!g.nonces.contains_key(&zero_nonce));
        assert_eq!(g.balance_of(&active), 40);
        assert_eq!(g.nonce_of(&active), 1);
        assert_eq!(
            g.cached_state_root,
            Some((0, compute_state_root(&g.balances, &g.nonces).unwrap()))
        );
    }

    #[test]
    fn restore_truncates_blocks_to_snapshot_height() {
        let (mut g, _blocks) = build_chain_n_with_snapshots(4, &[2]);
        assert_eq!(g.current_height(), 4);

        let restored = restore_latest_snapshot(&mut g, 4).unwrap();
        assert_eq!(restored, 2);
        assert_eq!(g.blocks.len(), 3, "genesis + block1 + block2");
        assert_eq!(g.current_height(), 2);
    }

    #[test]
    fn restore_rebuilds_canon_index() {
        let (mut g, blocks) = build_chain_n_with_snapshots(3, &[2]);
        g.canon_index.clear();

        restore_latest_snapshot(&mut g, 3).unwrap();
        assert!(g.canon_index.contains_key(blocks[0].hash()));
        assert!(g.canon_index.contains_key(blocks[1].hash()));
        assert!(g.canon_index.contains_key(blocks[2].hash()));
        assert!(!g.canon_index.contains_key(blocks[3].hash()));
    }

    #[test]
    fn restore_rebuilds_seen_blocks_so_replayed_tail_is_not_duplicate() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let mut blocks = vec![gen.clone()];
        let mut prev = gen.hash().to_string();
        let mut ts = gen.header.timestamp;

        for i in 1..=4 {
            ts += TARGET_BLOCK_TIME;
            let blk = make_test_block(&prev, i, ts, (0xA0 + i) as u8);
            apply_block(&mut g, &blk, None);
            prev = blk.hash().to_string();
            blocks.push(blk);
            if i == 2 {
                save_snapshot(&g, 2).unwrap();
            }
        }

        let restored = restore_latest_snapshot(&mut g, 4).unwrap();
        assert_eq!(restored, 2);
        assert!(!g.seen_blocks.contains(blocks[3].hash()));
        assert!(!g.seen_blocks.contains(blocks[4].hash()));

        assert_eq!(
            apply_block(&mut g, &blocks[3], None),
            crate::chain::accept::AcceptResult::CanonExtension { height: 3 }
        );
        assert_eq!(
            apply_block(&mut g, &blocks[4], None),
            crate::chain::accept::AcceptResult::CanonExtension { height: 4 }
        );
    }

    #[test]
    fn restore_rebuilds_cumulative_work() {
        let (mut g, blocks) = build_chain_n_with_snapshots(3, &[2]);
        g.cumulative_work.clear();

        restore_latest_snapshot(&mut g, 3).unwrap();
        let cw = g
            .cumulative_work
            .get(blocks[2].hash())
            .copied()
            .unwrap_or(0);
        assert_eq!(cw, 3 * crate::config::constants::DIFFICULTY_FLOOR as u128);
    }

    #[test]
    fn restore_rejects_snapshot_with_wrong_block_hash() {
        let (mut g, blocks) = build_chain_n(2);
        save_snapshot(&g, 2).unwrap();
        g.db.insert(
            b"meta:snapshot:2",
            bincode::serialize(&(2u64, blocks[1].hash())).unwrap(),
        )
        .unwrap();

        let result = restore_latest_snapshot(&mut g, 2);
        assert!(result.is_err());
    }

    #[test]
    fn restore_rejects_snapshot_with_correct_hash_but_wrong_block_root() {
        let (mut g, blocks) = build_chain_n(2);
        save_snapshot(&g, 2).unwrap();

        let key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let balances = BTreeMap::from([(key, 1u128)]);
        let nonces = BTreeMap::new();
        let wrong_root = compute_state_root(&balances, &nonces).unwrap();
        assert_ne!(wrong_root, blocks[2].header.state_root);

        g.db.insert(b"snap:balances:2", bincode::serialize(&balances).unwrap())
            .unwrap();
        g.db.insert(b"snap:nonces:2", bincode::serialize(&nonces).unwrap())
            .unwrap();
        g.db.insert(
            b"meta:snapshot_state_root:2",
            bincode::serialize(&wrong_root).unwrap(),
        )
        .unwrap();

        let result = restore_latest_snapshot(&mut g, 2);
        assert!(result.is_err());
    }

    #[test]
    fn restore_selects_newest_valid_canonical_snapshot() {
        let (mut g, blocks) = build_chain_n_with_snapshots(4, &[2, 4]);
        g.db.insert(
            b"meta:snapshot:4",
            bincode::serialize(&(4u64, blocks[3].hash())).unwrap(),
        )
        .unwrap();

        let restored = restore_latest_snapshot(&mut g, 4).unwrap();
        assert_eq!(restored, 2);
        assert_eq!(g.current_height(), 2);
        assert_eq!(
            g.cached_state_root,
            Some((2, blocks[2].header.state_root.clone()))
        );
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
        let count_before: usize = g.db.scan_prefix(b"meta:snapshot:").count();
        maybe_save_snapshot(&g).unwrap();
        let count_after: usize = g.db.scan_prefix(b"meta:snapshot:").count();
        assert_eq!(
            count_before, count_after,
            "should not save at non-multiple height"
        );
    }
}

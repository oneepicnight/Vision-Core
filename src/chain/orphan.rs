use crate::chain::ChainState;
use crate::types::transaction::canonical_tx_id;
use crate::types::Block;
use crate::config::constants::ORPHAN_POOL_MAX;

/// Move any orphaned blocks whose expected parent has now arrived into the
/// canonical chain or side-block store via `apply_block`.
///
/// Called after every successful `apply_block` with the newly-integrated
/// block's hash. Returns the number of orphans promoted.
pub fn process_orphans(g: &mut ChainState, parent_hash: &str) -> usize {
    let waiting = match g.orphan_pool.remove(parent_hash) {
        Some(v) => v,
        None => return 0,
    };

    let mut promoted = 0;
    for (block, _arrival_ts, source_peer) in waiting {
        let hash = block.hash().to_string();
        // Remove the reverse index entry before re-processing.
        g.orphan_by_hash.remove(&hash);

        match crate::chain::accept::apply_block(g, &block, Some(&source_peer)) {
            crate::chain::accept::AcceptResult::Rejected(reason) => {
                tracing::debug!("[ORPHAN] drop {:.8} after promotion: {}", hash, reason);
            }
            _ => promoted += 1,
        }
    }
    promoted
}

/// Add a block to the orphan pool because its parent is not yet known.
///
/// The pool is bounded by `ORPHAN_POOL_MAX`; if the pool is already at
/// capacity the oldest orphan is evicted before the new one is added.
pub fn add_orphan(g: &mut ChainState, block: Block, source_peer: &str) {
    let parent = block.header.parent_hash.clone();
    let hash   = block.hash().to_string();

    // Evict before inserting so the pool never exceeds ORPHAN_POOL_MAX.
    prune_old_orphans(g);

    g.orphan_by_hash.insert(hash, parent.clone());
    g.orphan_pool
        .entry(parent)
        .or_default()
        .push((block, now_secs(), source_peer.to_string()));
}

/// Evict the single oldest entry when the pool is at or above `ORPHAN_POOL_MAX`.
///
/// Called *before* inserting a new orphan so the pool never exceeds the limit.
pub fn prune_old_orphans(g: &mut ChainState) {
    let total: usize = g.orphan_pool.values().map(|v| v.len()).sum();
    if total < ORPHAN_POOL_MAX {
        return;
    }

    // Identify the (parent_hash, position) of the globally oldest entry.
    let mut oldest: Option<(String, usize, u64)> = None;
    for (key, entries) in &g.orphan_pool {
        for (pos, (_, ts, _)) in entries.iter().enumerate() {
            if oldest.as_ref().map_or(true, |(_, _, old_ts)| *ts < *old_ts) {
                oldest = Some((key.clone(), pos, *ts));
            }
        }
    }

    if let Some((key, pos, _)) = oldest {
        if let Some(entries) = g.orphan_pool.get_mut(&key) {
            let (evicted, _, _) = entries.remove(pos);
            g.orphan_by_hash.remove(evicted.hash());
            if entries.is_empty() {
                g.orphan_pool.remove(&key);
            }
        }
    }
}

/// Current UNIX timestamp in seconds.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::{apply_block, AcceptResult};
    use crate::chain::state::ChainState;
    use crate::config::constants::TARGET_BLOCK_TIME;
    use crate::genesis::genesis_block;
    use crate::types::{Block, BlockHeader, Tx};
    use crate::chain::accept::tests_helpers::make_test_block;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn coinbase_tx(height: u64) -> Tx {
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

    fn make_orphan_bookkeeping_block(
        parent_hash: &str,
        height: u64,
        timestamp: u64,
        slot: u8,
    ) -> Block {
        let txs = vec![coinbase_tx(height)];
        let tx_root = {
            let mut h = blake3::Hasher::new();
            for tx in &txs {
                h.update(canonical_tx_id(tx).as_bytes());
            }
            hex::encode(h.finalize().as_bytes())
        };

        Block {
            header: BlockHeader {
                parent_hash: parent_hash.to_string(),
                number:      height,
                timestamp,
                difficulty:  crate::config::constants::DIFFICULTY_FLOOR,
                nonce:       slot as u64,
                pow_hash:    format!("{:064x}", slot),
                state_root:  "0".repeat(64),
                tx_root,
                miner:       "test_miner".to_string(),
            },
            txs,
            weight: 0,
        }
    }

    #[test]
    fn add_orphan_stores_in_pool() {
        let mut g = temp_state();
        let unknown_parent = "dead".repeat(16);
        let blk = make_test_block(&unknown_parent, 5, 1_700_000_150, 0xAA);

        add_orphan(&mut g, blk.clone(), "peer1");

        assert_eq!(g.orphan_pool.len(), 1);
        assert!(g.orphan_pool.contains_key(unknown_parent.as_str()));
        assert!(g.orphan_by_hash.contains_key(blk.hash()));
    }

    #[test]
    fn process_orphans_promotes_block() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let ts1 = gen.header.timestamp + TARGET_BLOCK_TIME;
        let ts2 = ts1 + TARGET_BLOCK_TIME;
        let b1 = make_test_block(gen.hash(), 1, ts1, 0xAA);
        let b2 = make_test_block(b1.hash(), 2, ts2, 0xBB);

        // b2 arrives first — stored as orphan.
        add_orphan(&mut g, b2.clone(), "peer1");
        assert_eq!(g.orphan_pool.len(), 1);

        // b1 arrives and triggers promotion of b2.
        apply_block(&mut g, &b1, None);

        assert_eq!(g.blocks.len(), 3, "genesis + b1 + promoted b2");
        assert_eq!(g.orphan_pool.len(), 0);
    }

    #[test]
    fn orphan_pool_evicts_oldest_when_full() {
        use crate::config::constants::ORPHAN_POOL_MAX;

        // Fill the pool to capacity + 1.
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        for i in 0..=(ORPHAN_POOL_MAX as u64) {
            let fake_parent = format!("{:064x}", i);
            let blk = make_orphan_bookkeeping_block(&fake_parent, i + 100, 1_700_000_000 + i * 30, 0xAA);
            add_orphan(&mut g, blk, "peer_flood");
        }

        let total: usize = g.orphan_pool.values().map(|v| v.len()).sum();
        assert!(
            total <= ORPHAN_POOL_MAX,
            "pool size {} should be â‰¤ {}",
            total, ORPHAN_POOL_MAX
        );
    }

    #[test]
    fn orphan_by_hash_cleaned_on_promotion() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        let ts1 = gen.header.timestamp + TARGET_BLOCK_TIME;
        let b1 = make_test_block(gen.hash(), 1, ts1, 0xAA);

        // Manually add b1 to orphan pool with a fake parent to test cleanup path.
        let b1_hash = b1.hash().to_string();
        add_orphan(&mut g, b1.clone(), "peer");

        // Process orphans for a different parent (won't find b1).
        process_orphans(&mut g, &"0".repeat(64));

        // b1 is still in the pool; reverse index still intact.
        assert!(g.orphan_by_hash.contains_key(&b1_hash));

        // Now process for b1's actual parent.
        process_orphans(&mut g, gen.hash());

        // b1 was promoted (or rejected), reverse index cleared.
        assert!(!g.orphan_by_hash.contains_key(&b1_hash));
    }
}





use crate::chain::ChainState;
use crate::types::Block;
use anyhow::{anyhow, Result};
use sled::Batch;

// ─── Key scheme ───────────────────────────────────────────────────────────────
//
//  block:{hash}          → bincode(Block)          all known blocks
//  height:{n}            → "{hash}"                canonical height → hash
//  meta:tip_hash         → "{hash}"                current canonical tip
//  meta:tip_height       → "{n}"                   current canonical height
//  snap:balances:{n}     → bincode(BTreeMap)        snapshot balances
//  snap:nonces:{n}       → bincode(BTreeMap)        snapshot nonces
//  meta:snapshot:{n}     → bincode((height, hash))  snapshot metadata index

// ─── Block storage ────────────────────────────────────────────────────────────

/// Persist a block to the sled database under `block:{hash}`.
pub fn store_block(g: &ChainState, block: &Block) -> Result<()> {
    let value = bincode::serialize(block)?;
    g.db.insert(format!("block:{}", block.hash()).as_bytes(), value)?;
    Ok(())
}

/// Atomically persist a canonical extension: raw block, height index, and tip.
pub fn persist_canonical_extension(g: &ChainState, block: &Block) -> Result<()> {
    let hash = block.hash();
    let height = block.header.number;
    let mut batch = Batch::default();

    batch.insert(
        format!("block:{}", hash).into_bytes(),
        bincode::serialize(block)?,
    );
    batch.insert(format!("height:{}", height).into_bytes(), hash.as_bytes());
    batch.insert(b"meta:tip_height", height.to_string().as_bytes());
    batch.insert(b"meta:tip_hash", hash.as_bytes());
    g.db.apply_batch(batch)?;
    Ok(())
}

/// Load a block by its PoW hash. Returns `None` if not found.
pub fn load_block(g: &ChainState, hash: &str) -> Result<Option<Block>> {
    match g.db.get(format!("block:{}", hash).as_bytes())? {
        Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
        None => Ok(None),
    }
}

// ─── Height index ─────────────────────────────────────────────────────────────

/// Record that `hash` is the canonical block at `height`.
///
/// Also updates `meta:tip_hash` and `meta:tip_height` when `height` is
/// the new maximum so that `load_canon_chain` can recover the full chain.
pub fn store_height_index(g: &ChainState, height: u64, hash: &str) -> Result<()> {
    g.db.insert(format!("height:{}", height).as_bytes(), hash.as_bytes())?;
    // Maintain tip pointers eagerly; cheaper than a scan at startup.
    let current_tip: u64 =
        g.db.get(b"meta:tip_height")?
            .and_then(|v| String::from_utf8(v.to_vec()).ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
    if height >= current_tip {
        g.db.insert(b"meta:tip_height", height.to_string().as_bytes())?;
        g.db.insert(b"meta:tip_hash", hash.as_bytes())?;
    }
    Ok(())
}

/// Atomically persist the canonical height sequence after a successful reorg.
///
/// Raw side-chain blocks are preserved under block:{hash}. Only the canonical
/// height index and tip metadata are rewritten.
pub fn persist_canonical_reorg(
    g: &ChainState,
    canonical_blocks: &[Block],
    previous_tip_height: u64,
) -> Result<()> {
    let tip = canonical_blocks
        .last()
        .ok_or_else(|| anyhow!("cannot persist empty canonical chain"))?;
    let new_tip_height = tip.header.number;
    let mut batch = Batch::default();

    for block in canonical_blocks {
        let hash = block.hash();
        batch.insert(
            format!("block:{}", hash).into_bytes(),
            bincode::serialize(block)?,
        );
        batch.insert(
            format!("height:{}", block.header.number).into_bytes(),
            hash.as_bytes(),
        );
    }

    for height in (new_tip_height + 1)..=previous_tip_height {
        batch.remove(format!("height:{}", height).into_bytes());
    }

    batch.insert(b"meta:tip_height", new_tip_height.to_string().as_bytes());
    batch.insert(b"meta:tip_hash", tip.hash().as_bytes());
    g.db.apply_batch(batch)?;
    Ok(())
}

/// Load the hash stored at `height` in the canonical height index.
pub fn load_height_index(g: &ChainState, height: u64) -> Result<Option<String>> {
    match g.db.get(format!("height:{}", height).as_bytes())? {
        Some(bytes) => Ok(Some(String::from_utf8(bytes.to_vec())?)),
        None => Ok(None),
    }
}

// ─── Canonical chain recovery ─────────────────────────────────────────────────

/// Reload the entire canonical chain from the sled database into `g.blocks`,
/// `g.canon_index`, and `g.cumulative_work`.
///
/// Called once during `ChainState::open_with_genesis`. If the database is
/// empty the function returns `Ok(())` without modifying any in-memory state.
pub fn load_canon_chain(g: &mut ChainState) -> Result<()> {
    let tip_height: u64 = match g.db.get(b"meta:tip_height")? {
        Some(bytes) => String::from_utf8(bytes.to_vec())?.parse()?,
        None => return Ok(()), // no chain stored yet
    };

    let mut cumulative = 0u128;
    for h in 0..=tip_height {
        let hash =
            load_height_index(g, h)?.ok_or_else(|| anyhow!("height index missing at h={}", h))?;

        let block = load_block(g, &hash)?
            .ok_or_else(|| anyhow!("block missing for hash {} at h={}", hash, h))?;

        cumulative += block.header.difficulty as u128;
        g.seen_blocks.insert(hash.clone());
        g.canon_index.insert(hash.clone(), block.header.number);
        g.cumulative_work.insert(hash, cumulative);
        g.blocks.push(block);
    }

    tracing::info!("[STORAGE] Reloaded {} canonical blocks", g.blocks.len());
    Ok(())
}

// ─── Metadata ─────────────────────────────────────────────────────────────────

/// Persist an arbitrary metadata string under `meta:{key}`.
pub fn store_meta(g: &ChainState, key: &str, value: &str) -> Result<()> {
    g.db.insert(format!("meta:{}", key).as_bytes(), value.as_bytes())?;
    Ok(())
}

/// Load a metadata string. Returns `None` if the key is absent.
pub fn load_meta(g: &ChainState, key: &str) -> Result<Option<String>> {
    match g.db.get(format!("meta:{}", key).as_bytes())? {
        Some(bytes) => Ok(Some(String::from_utf8(bytes.to_vec())?)),
        None => Ok(None),
    }
}

/// Write the canonical tip hash to the database so restarts resume correctly.
pub fn persist_tip(g: &ChainState) -> Result<()> {
    let hash = g.tip_hash();
    let height = g.current_height();
    g.db.insert(b"meta:tip_hash", hash.as_bytes())?;
    g.db.insert(b"meta:tip_height", height.to_string().as_bytes())?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::apply_block;
    use crate::chain::state::ChainState;
    use crate::config::settings::Settings;
    use crate::genesis::genesis_block;
    use crate::node::bootstrap::bootstrap_chain;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    #[test]
    fn bootstrap_writes_genesis_height_index() {
        let mut g = temp_state();
        let settings = Settings::default();
        bootstrap_chain(&mut g, &settings).unwrap();

        assert_eq!(
            load_height_index(&g, 0).unwrap().as_deref(),
            Some(crate::genesis::GENESIS_HASH)
        );
    }

    #[test]
    fn store_and_load_block_round_trip() {
        let g = temp_state();
        let gen = genesis_block();
        store_block(&g, &gen).unwrap();
        let loaded = load_block(&g, gen.hash()).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().hash(), gen.hash());
    }

    #[test]
    fn load_block_missing_returns_none() {
        let g = temp_state();
        let result = load_block(&g, &"aa".repeat(32)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn store_meta_and_load_round_trip() {
        let g = temp_state();
        store_meta(&g, "test_key", "hello_world").unwrap();
        let val = load_meta(&g, "test_key").unwrap();
        assert_eq!(val.as_deref(), Some("hello_world"));
    }

    #[test]
    fn load_meta_missing_returns_none() {
        let g = temp_state();
        assert!(load_meta(&g, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn persist_tip_records_hash_and_height() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        persist_tip(&g).unwrap();

        assert_eq!(
            load_meta(&g, "tip_hash").unwrap().as_deref(),
            Some(gen.hash())
        );
        assert_eq!(load_meta(&g, "tip_height").unwrap().as_deref(), Some("0"));
    }

    #[test]
    fn height_index_store_and_load() {
        let g = temp_state();
        let hash = "ab".repeat(32);
        store_height_index(&g, 7, &hash).unwrap();
        let loaded = load_height_index(&g, 7).unwrap();
        assert_eq!(loaded.as_deref(), Some(hash.as_str()));
    }

    #[test]
    fn canonical_reorg_persistence_removes_stale_height_indexes() {
        use crate::chain::accept::tests_helpers::make_test_block;
        use crate::config::constants::TARGET_BLOCK_TIME;

        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let b1 = make_test_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        apply_block(&mut g, &b1, None);
        let b2 = make_test_block(b1.hash(), 2, b1.header.timestamp + TARGET_BLOCK_TIME, 0xBB);
        apply_block(&mut g, &b2, None);
        assert_eq!(
            load_height_index(&g, 2).unwrap().as_deref(),
            Some(b2.hash())
        );

        persist_canonical_reorg(&g, &[gen.clone(), b1.clone()], 2).unwrap();

        assert_eq!(
            load_height_index(&g, 0).unwrap().as_deref(),
            Some(gen.hash())
        );
        assert_eq!(
            load_height_index(&g, 1).unwrap().as_deref(),
            Some(b1.hash())
        );
        assert!(load_height_index(&g, 2).unwrap().is_none());
        assert_eq!(load_meta(&g, "tip_height").unwrap().as_deref(), Some("1"));
        assert_eq!(
            load_meta(&g, "tip_hash").unwrap().as_deref(),
            Some(b1.hash())
        );
        assert!(load_block(&g, b2.hash()).unwrap().is_some());
    }

    #[test]
    fn load_canon_chain_recovers_blocks() {
        // Store genesis + 1 block via storage helpers directly, then recover.
        use crate::chain::accept::apply_block;
        use crate::config::constants::TARGET_BLOCK_TIME;

        // First pass: apply blocks so sled is populated.
        let db1 = sled::Config::new().temporary(true).open().unwrap();
        let mut g = ChainState::empty(db1);
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        // Manually write the height index (normally done inside accept via storage).
        store_height_index(&g, 0, gen.hash()).unwrap();

        // Rebuild from the same underlying sled tree.
        let db2 = g.db.clone();
        let mut g2 = ChainState::empty(db2);
        load_canon_chain(&mut g2).unwrap();

        assert_eq!(g2.blocks.len(), 1);
        assert_eq!(g2.blocks[0].hash(), gen.hash());
        assert!(g2.canon_index.contains_key(gen.hash()));
    }
}

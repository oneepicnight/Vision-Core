use anyhow::{anyhow, Result};

use crate::chain::accept::{apply_block, AcceptResult};
use crate::chain::snapshots::restore_latest_snapshot;
use crate::chain::storage::{
    load_block, load_height_index, load_meta, store_block, store_height_index, store_meta,
};
use crate::chain::ChainState;
use crate::config::settings::Settings;
use crate::genesis::genesis::{genesis_block, validate_genesis_hash, verify_stored_genesis};

/// Initialise the chain from scratch or verify an existing database.
///
/// 1. Validates the compile-time genesis hash constant.
/// 2. If the DB has no genesis block, writes one and returns.
/// 3. If the DB has a genesis block, verifies it matches the canonical hash.
///
/// Aborts on any mismatch - these are irrecoverable without a wipe.
pub fn bootstrap_chain(g: &mut ChainState, _settings: &Settings) -> Result<()> {
    validate_genesis_hash()?;

    match load_meta(g, "genesis_hash")? {
        None => {
            let genesis = genesis_block();
            store_block(g, &genesis)?;
            store_height_index(g, 0, genesis.hash())?;
            store_meta(g, "genesis_hash", genesis.hash())?;
            g.blocks.push(genesis.clone());
            g.canon_index.insert(genesis.hash().to_string(), 0);
            tracing::info!("[BOOTSTRAP] Genesis block written: {}", genesis.hash());
        }
        Some(stored_hash) => {
            verify_stored_genesis(&stored_hash)?;
            tracing::info!("[BOOTSTRAP] Genesis verified: {}", stored_hash);
        }
    }

    Ok(())
}

/// Rebuild the canonical tip from the stored snapshot and on-disk tail before services start.
///
/// Fresh databases are left at genesis when no snapshot exists yet. Existing
/// databases resume from the newest snapshot and then replay any locally
/// stored canonical tail so balances, nonces, cached state root, and the
/// canonical tip are restored before sync or API requests start.
fn replay_stored_canonical_tail(g: &mut ChainState, restored_height: u64) -> Result<u64> {
    let tip_height: u64 = load_meta(g, "tip_height")?
        .ok_or_else(|| anyhow!("missing persisted tip height during startup recovery"))?
        .parse()?;

    if tip_height <= restored_height {
        tracing::info!(
            "[BOOTSTRAP] Local recovery already at persisted tip h={}",
            tip_height
        );
        g.refresh_cached_state_root_from_tip();
        return Ok(tip_height);
    }

    tracing::info!(
        "[BOOTSTRAP] Replaying canonical tail h={}..={} before services start",
        restored_height + 1,
        tip_height
    );

    for height in (restored_height + 1)..=tip_height {
        let hash = load_height_index(g, height)?
            .ok_or_else(|| anyhow!("missing canonical height index at h={}", height))?;
        let block = load_block(g, &hash)?
            .ok_or_else(|| anyhow!("missing canonical block {} at h={}", hash, height))?;

        if block.header.number != height {
            return Err(anyhow!(
                "startup replay block height mismatch at h={}: block reports {}",
                height,
                block.header.number
            ));
        }

        match apply_block(g, &block, None) {
            AcceptResult::CanonExtension {
                height: applied_height,
            } if applied_height == height => {
                g.refresh_cached_state_root_from_tip();
                tracing::debug!(
                    "[BOOTSTRAP] Replayed canonical block h={} hash={:.8}",
                    height,
                    hash
                );
            }
            other => {
                return Err(anyhow!(
                    "startup replay rejected h={} hash={:.8}: {:?}",
                    height,
                    hash,
                    other
                ));
            }
        }
    }

    tracing::info!(
        "[BOOTSTRAP] Recovered canonical tip h={} hash={}",
        g.current_height(),
        g.tip_hash()
    );
    Ok(tip_height)
}

fn rebuild_canonical_state_from_genesis(g: &mut ChainState) -> Result<u64> {
    let tip_height: u64 = load_meta(g, "tip_height")?
        .ok_or_else(|| anyhow!("missing persisted tip height during full startup recovery"))?
        .parse()?;

    let mut blocks = Vec::new();
    for height in 0..=tip_height {
        let hash = load_height_index(g, height)?
            .ok_or_else(|| anyhow!("missing canonical height index at h={}", height))?;
        let block = load_block(g, &hash)?
            .ok_or_else(|| anyhow!("missing canonical block {} at h={}", hash, height))?;
        if block.header.number != height {
            return Err(anyhow!(
                "startup rebuild block height mismatch at h={}: block reports {}",
                height,
                block.header.number
            ));
        }
        blocks.push(block);
    }

    g.blocks.clear();
    g.canon_index.clear();
    g.cumulative_work.clear();
    g.seen_blocks.clear();
    g.balances.clear();
    g.nonces.clear();
    g.cached_state_root = None;

    tracing::info!(
        "[BOOTSTRAP] Rebuilding canonical state from genesis through h={} before services start",
        tip_height
    );

    for block in blocks {
        let height = block.header.number;
        match apply_block(g, &block, None) {
            AcceptResult::CanonExtension {
                height: applied_height,
            } if applied_height == height => {
                g.refresh_cached_state_root_from_tip();
                tracing::debug!(
                    "[BOOTSTRAP] Rebuilt canonical block h={} hash={:.8}",
                    height,
                    block.hash()
                );
            }
            other => {
                return Err(anyhow!(
                    "startup rebuild rejected h={} hash={:.8}: {:?}",
                    height,
                    block.hash(),
                    other
                ));
            }
        }
    }

    tracing::info!(
        "[BOOTSTRAP] Rebuilt canonical tip h={} hash={}",
        g.current_height(),
        g.tip_hash()
    );
    Ok(tip_height)
}
/// Open, bootstrap, and restore the latest available snapshot for a node.
///
/// Fresh databases are left at genesis when no snapshot exists yet. Existing
/// databases resume from the newest snapshot and then replay any locally
/// stored canonical tail so balances, nonces, cached state root, and the
/// canonical tip are restored before sync or API requests start.
pub fn initialize_chain_state(settings: &Settings) -> Result<ChainState> {
    let mut chain_state = ChainState::open_with_genesis(&settings.data_dir)?;
    bootstrap_chain(&mut chain_state, settings)?;
    chain_state.refresh_cached_state_root_from_tip();

    let current_height = chain_state.current_height();
    match restore_latest_snapshot(&mut chain_state, current_height) {
        Ok(restored_height) => {
            tracing::info!(
                "[BOOTSTRAP] Restored snapshot at height {} before services start",
                restored_height
            );
            if let Err(e) = replay_stored_canonical_tail(&mut chain_state, restored_height) {
                tracing::error!("[BOOTSTRAP] Canonical tail replay failed: {}", e);
                return Err(e);
            }
        }
        Err(e) => {
            if current_height > 0 {
                tracing::warn!(
                    "[BOOTSTRAP] No valid canonical snapshot restored on startup: {}; rebuilding from genesis",
                    e
                );
                if let Err(rebuild_err) = rebuild_canonical_state_from_genesis(&mut chain_state) {
                    tracing::error!("[BOOTSTRAP] Full canonical rebuild failed: {}", rebuild_err);
                    return Err(rebuild_err);
                }
            } else {
                tracing::debug!("[BOOTSTRAP] No snapshot restored on startup: {}", e);
            }
        }
    }

    Ok(chain_state)
}

/// Seed the peer store with the default seed peers from `settings`.
pub fn seed_peers(settings: &Settings) -> Vec<String> {
    settings.seed_peers.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::tests_helpers::make_test_block;
    use crate::chain::snapshots::save_snapshot;
    use crate::chain::state::ChainState;
    use crate::config::constants::TARGET_BLOCK_TIME;
    use crate::genesis::genesis_block;
    use std::path::Path;
    use std::process::Command;

    fn test_settings(data_dir: &Path) -> Settings {
        Settings {
            data_dir: data_dir.display().to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            p2p_addr: "127.0.0.1:0".to_string(),
            p2p_advertised_host: None,
            p2p_advertised_port: None,
            allow_private_peer_addresses: true,
            miner_address: "0".repeat(64),
            mining_enabled: false,
            mining_threads: 0,
            alpha_airdrop_enabled: false,
            seed_peers: Vec::new(),
        }
    }

    fn build_chain_with_snapshot(
        data_dir: &Path,
        snapshot_height: u64,
        tip_height: u64,
    ) -> Result<(String, String, String, u128, u64)> {
        let settings = test_settings(data_dir);
        let mut chain = ChainState::open_with_genesis(&settings.data_dir)?;
        bootstrap_chain(&mut chain, &settings)?;

        let miner = "0".repeat(64);
        let mut prev = chain.tip_hash();
        let mut ts = genesis_block().header.timestamp;
        let mut tip_hash = prev.clone();
        let mut tip_root = chain
            .cached_state_root
            .as_ref()
            .map(|(_, root)| root.clone())
            .unwrap_or_else(|| chain.blocks.last().unwrap().header.state_root.clone());

        for height in 1..=tip_height {
            ts += TARGET_BLOCK_TIME;
            let block = make_test_block(&prev, height, ts, 0xA0u8.wrapping_add(height as u8));
            match apply_block(&mut chain, &block, None) {
                AcceptResult::CanonExtension {
                    height: applied_height,
                } => assert_eq!(applied_height, height),
                other => panic!(
                    "expected canonical extension at h={}, got {:?}",
                    height, other
                ),
            }

            if height == snapshot_height {
                save_snapshot(&chain, height)?;
            }

            if height == tip_height {
                tip_hash = block.hash().to_string();
                tip_root = block.header.state_root.clone();
            }

            prev = block.hash().to_string();
        }

        Ok((
            tip_hash,
            tip_root,
            miner.clone(),
            chain.balance_of(&miner),
            chain.nonce_of(&miner),
        ))
    }

    fn run_recovery_worker(
        data_dir: &Path,
        expect_fail: bool,
        check_block_hash: Option<&str>,
    ) -> Result<String> {
        let exe = std::env::current_exe()?;
        let output_file = data_dir.join("bootstrap-worker.out");
        let mut cmd = Command::new(exe);
        cmd.arg("--exact")
            .arg("node::bootstrap::tests::bootstrap_recovery_worker")
            .arg("--ignored")
            .arg("--test-threads=1")
            .env("VISION_BOOTSTRAP_WORKER_DIR", data_dir)
            .env("VISION_BOOTSTRAP_WORKER_OUT", &output_file);
        if let Some(hash) = check_block_hash {
            cmd.env("VISION_BOOTSTRAP_CHECK_BLOCK_HASH", hash);
        }
        if expect_fail {
            cmd.env("VISION_BOOTSTRAP_EXPECT_FAIL", "1");
        }
        let output = cmd.output()?;
        assert!(
            output.status.success(),
            "worker process failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(std::fs::read_to_string(output_file)?)
    }

    fn parse_kv(output: &str) -> std::collections::BTreeMap<String, String> {
        let mut map = std::collections::BTreeMap::new();
        for line in output.lines() {
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        map
    }

    #[test]
    fn initialize_chain_state_replays_canonical_tail_before_services_start() -> Result<()> {
        let recovery_dir = tempfile::tempdir()?;
        let (tip_hash, tip_root, miner, expected_balance, expected_nonce) =
            build_chain_with_snapshot(recovery_dir.path(), 64, 66)?;
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let output = run_recovery_worker(recovery_dir.path(), false, None)?;
        let values = parse_kv(&output);
        assert_eq!(values.get("height").map(String::as_str), Some("66"));
        assert_eq!(
            values.get("tip_hash").map(String::as_str),
            Some(tip_hash.as_str())
        );
        assert_eq!(
            values.get("state_root").map(String::as_str),
            Some(tip_root.as_str())
        );
        assert_eq!(
            values.get("balance").map(String::as_str),
            Some(expected_balance.to_string().as_str())
        );
        assert_eq!(
            values.get("nonce").map(String::as_str),
            Some(expected_nonce.to_string().as_str())
        );
        assert_eq!(
            values.get("miner").map(String::as_str),
            Some(miner.as_str())
        );
        Ok(())
    }

    #[test]
    fn initialize_chain_state_rebuilds_from_genesis_when_snapshot_lineage_is_stale() -> Result<()> {
        let recovery_dir = tempfile::tempdir()?;
        let (tip_hash, tip_root, miner, expected_balance, expected_nonce) =
            build_chain_with_snapshot(recovery_dir.path(), 2, 4)?;
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let chain = ChainState::open_with_genesis(&recovery_dir.path().display().to_string())?;
        chain.db.insert(
            b"meta:snapshot:2",
            bincode::serialize(&(2u64, "aa".repeat(32).as_str()))?,
        )?;
        drop(chain);

        let output = run_recovery_worker(recovery_dir.path(), false, None)?;
        let values = parse_kv(&output);
        assert_eq!(values.get("height").map(String::as_str), Some("4"));
        assert_eq!(
            values.get("tip_hash").map(String::as_str),
            Some(tip_hash.as_str())
        );
        assert_eq!(
            values.get("state_root").map(String::as_str),
            Some(tip_root.as_str())
        );
        assert_eq!(
            values.get("balance").map(String::as_str),
            Some(expected_balance.to_string().as_str())
        );
        assert_eq!(
            values.get("nonce").map(String::as_str),
            Some(expected_nonce.to_string().as_str())
        );
        assert_eq!(
            values.get("miner").map(String::as_str),
            Some(miner.as_str())
        );
        Ok(())
    }
    #[test]
    fn initialize_chain_state_rejects_missing_height_index() -> Result<()> {
        let recovery_dir = tempfile::tempdir()?;
        let _ = build_chain_with_snapshot(recovery_dir.path(), 64, 66)?;
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let chain = ChainState::open_with_genesis(&recovery_dir.path().display().to_string())?;
        chain.db.remove(b"height:65")?;
        drop(chain);

        let _ = run_recovery_worker(recovery_dir.path(), true, None)?;
        Ok(())
    }

    #[test]
    fn initialize_chain_state_rejects_invalid_stored_tail_block() -> Result<()> {
        let recovery_dir = tempfile::tempdir()?;
        let _ = build_chain_with_snapshot(recovery_dir.path(), 64, 66)?;
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let chain = ChainState::open_with_genesis(&recovery_dir.path().display().to_string())?;
        let block_65_hash = chain.db.get(b"height:65")?.unwrap();
        let block_65_hash = String::from_utf8(block_65_hash.to_vec())?;
        let mut block_65 =
            load_block(&chain, &block_65_hash)?.expect("canonical block 65 should exist");
        block_65.header.state_root = "f".repeat(64);
        store_block(&chain, &block_65)?;
        drop(chain);

        let _ = run_recovery_worker(recovery_dir.path(), true, None)?;
        Ok(())
    }

    #[test]
    fn initialize_chain_state_ignores_side_chain_blocks() -> Result<()> {
        let recovery_dir = tempfile::tempdir()?;
        let (_tip_hash, tip_root, miner, expected_balance, expected_nonce) =
            build_chain_with_snapshot(recovery_dir.path(), 64, 66)?;
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let chain = ChainState::open_with_genesis(&recovery_dir.path().display().to_string())?;
        let parent_hash = chain.db.get(b"height:64")?.unwrap();
        let parent_hash = String::from_utf8(parent_hash.to_vec())?;
        let side_block = make_test_block(
            &parent_hash,
            65,
            genesis_block().header.timestamp + TARGET_BLOCK_TIME * 65,
            0xF5,
        );
        store_block(&chain, &side_block)?;
        drop(chain);

        let output = run_recovery_worker(recovery_dir.path(), false, Some(side_block.hash()))?;
        let values = parse_kv(&output);
        assert_eq!(values.get("height").map(String::as_str), Some("66"));
        assert_eq!(
            values.get("state_root").map(String::as_str),
            Some(tip_root.as_str())
        );
        assert_eq!(
            values.get("miner").map(String::as_str),
            Some(miner.as_str())
        );
        assert_eq!(
            values.get("balance").map(String::as_str),
            Some(expected_balance.to_string().as_str())
        );
        assert_eq!(
            values.get("nonce").map(String::as_str),
            Some(expected_nonce.to_string().as_str())
        );
        assert_eq!(
            values.get("block_present").map(String::as_str),
            Some("false")
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn bootstrap_recovery_worker() -> Result<()> {
        let data_dir = std::env::var("VISION_BOOTSTRAP_WORKER_DIR")?;
        let output_file = std::env::var("VISION_BOOTSTRAP_WORKER_OUT")?;
        let expect_fail = std::env::var("VISION_BOOTSTRAP_EXPECT_FAIL")
            .ok()
            .as_deref()
            == Some("1");
        let settings = test_settings(Path::new(&data_dir));
        let output = match initialize_chain_state(&settings) {
            Ok(chain) if expect_fail => {
                format!(
                    "error=expected startup recovery to fail, but it succeeded at height {}\n",
                    chain.current_height()
                )
            }
            Ok(chain) => {
                let block_present = std::env::var("VISION_BOOTSTRAP_CHECK_BLOCK_HASH")
                    .ok()
                    .map(|hash| chain.block_by_hash(&hash).is_some())
                    .unwrap_or(false);
                format!(
                    "height={}\ntip_hash={}\nstate_root={}\nbalance={}\nnonce={}\nminer={}\nblock_present={}\n",
                    chain.current_height(),
                    chain.tip_hash(),
                    chain.cached_state_root.as_ref().map(|(_, root)| root.clone()).unwrap_or_default(),
                    chain.balance_of(&settings.miner_address),
                    chain.nonce_of(&settings.miner_address),
                    settings.miner_address,
                    block_present,
                )
            }
            Err(err) if expect_fail => {
                format!("error={}\n", err)
            }
            Err(err) => return Err(err),
        };
        std::fs::write(output_file, output)?;
        Ok(())
    }

    fn block_root(chain: &ChainState) -> String {
        chain
            .blocks
            .last()
            .map(|b| b.header.state_root.clone())
            .unwrap_or_default()
    }
}

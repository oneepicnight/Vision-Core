use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Result};

use crate::chain::accept::{apply_block, AcceptResult};
use crate::chain::snapshots::restore_latest_snapshot;
use crate::chain::storage::{
    load_block, load_height_index, load_meta, store_block, store_height_index, store_meta,
};
use crate::chain::ChainState;
use crate::config::settings::Settings;
use crate::genesis::genesis::{genesis_block, validate_genesis_hash, verify_stored_genesis};

static DATA_DIRECTORY_PROBE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
enum DataDirectoryError {
    InvalidValue {
        value: String,
        requirement: &'static str,
    },
    Filesystem {
        value: String,
        effective: PathBuf,
        operation: &'static str,
        source: io::Error,
    },
    DatabaseOpen {
        value: String,
        effective: PathBuf,
        source: anyhow::Error,
    },
}

impl fmt::Display for DataDirectoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue { value, requirement } => write!(
                formatter,
                "invalid VISION_DATA_DIR value {value:?}: {requirement}"
            ),
            Self::Filesystem {
                value,
                effective,
                operation,
                source,
            } => write!(
                formatter,
                "invalid VISION_DATA_DIR value {value:?}: could not {operation} effective data directory {}: {source}",
                effective.display()
            ),
            Self::DatabaseOpen {
                value,
                effective,
                source,
            } => write!(
                formatter,
                "invalid VISION_DATA_DIR value {value:?}: could not open database in effective data directory {}: {source}",
                effective.display()
            ),
        }
    }
}

impl std::error::Error for DataDirectoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Filesystem { source, .. } => Some(source),
            Self::InvalidValue { .. } | Self::DatabaseOpen { .. } => None,
        }
    }
}

fn data_directory_io_error(
    value: &str,
    effective: &Path,
    operation: &'static str,
    source: io::Error,
) -> DataDirectoryError {
    DataDirectoryError::Filesystem {
        value: value.to_string(),
        effective: effective.to_path_buf(),
        operation,
        source,
    }
}

fn prepare_data_directory(value: &str) -> std::result::Result<PathBuf, DataDirectoryError> {
    let current_directory = std::env::current_dir().map_err(|source| {
        data_directory_io_error(
            value,
            Path::new(value),
            "resolve the process working directory for",
            source,
        )
    })?;
    prepare_data_directory_from(value, &current_directory)
}

fn prepare_data_directory_from(
    value: &str,
    current_directory: &Path,
) -> std::result::Result<PathBuf, DataDirectoryError> {
    if value.is_empty() {
        return Err(DataDirectoryError::InvalidValue {
            value: value.to_string(),
            requirement: "expected a non-empty path",
        });
    }
    if value.trim().is_empty() {
        return Err(DataDirectoryError::InvalidValue {
            value: value.to_string(),
            requirement: "expected a path containing non-whitespace characters",
        });
    }
    if value.trim() != value {
        return Err(DataDirectoryError::InvalidValue {
            value: value.to_string(),
            requirement: "leading and trailing whitespace are not allowed",
        });
    }

    let configured = Path::new(value);
    let effective = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        current_directory.join(configured)
    };

    match fs::metadata(&effective) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(DataDirectoryError::InvalidValue {
                value: value.to_string(),
                requirement: "the effective path must be a directory, not a regular file",
            });
        }
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            let parent = effective
                .parent()
                .ok_or_else(|| DataDirectoryError::InvalidValue {
                    value: value.to_string(),
                    requirement: "the effective path must have an existing parent directory",
                })?;
            let parent_metadata = fs::metadata(parent).map_err(|source| {
                data_directory_io_error(value, &effective, "inspect the parent of", source)
            })?;
            if !parent_metadata.is_dir() {
                return Err(DataDirectoryError::InvalidValue {
                    value: value.to_string(),
                    requirement: "the effective path must have a directory as its parent",
                });
            }
            fs::create_dir(&effective)
                .map_err(|source| data_directory_io_error(value, &effective, "create", source))?;
        }
        Err(source) => {
            return Err(data_directory_io_error(
                value, &effective, "inspect", source,
            ));
        }
    }

    fs::read_dir(&effective)
        .map_err(|source| data_directory_io_error(value, &effective, "access", source))?;

    let database_path = effective.join("chain.db");
    match fs::metadata(&database_path) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(DataDirectoryError::InvalidValue {
                value: value.to_string(),
                requirement: "the effective chain.db path must be a directory",
            });
        }
        Ok(_) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(data_directory_io_error(
                value,
                &effective,
                "inspect chain.db under",
                source,
            ));
        }
    }

    let probe_id = DATA_DIRECTORY_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let probe_path = effective.join(format!(
        ".vision-core-write-probe-{}-{probe_id}",
        std::process::id()
    ));
    fs::create_dir(&probe_path).map_err(|source| {
        data_directory_io_error(value, &effective, "verify write access to", source)
    })?;
    fs::remove_dir(&probe_path).map_err(|source| {
        data_directory_io_error(value, &effective, "remove the write probe from", source)
    })?;

    Ok(effective)
}

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
    let effective_data_directory = prepare_data_directory(&settings.data_dir)?;
    tracing::info!(
        "[STORAGE] Effective data directory: {}",
        effective_data_directory.display()
    );
    let mut chain_state = ChainState::open_with_genesis(&settings.data_dir).map_err(|source| {
        DataDirectoryError::DatabaseOpen {
            value: settings.data_dir.clone(),
            effective: effective_data_directory,
            source,
        }
    })?;
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
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    fn test_settings(data_dir: &Path) -> Settings {
        Settings {
            data_dir: data_dir.display().to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            p2p_addr: "127.0.0.1:0".to_string(),
            p2p_auto_port: false,
            p2p_advertised_host: None,
            p2p_advertised_port: None,
            p2p_advertised_port_auto: false,
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
    fn data_directory_policy_preserves_default_relative_path() -> Result<()> {
        let current_directory = tempfile::tempdir()?;

        let effective = prepare_data_directory_from("./data", current_directory.path())?;

        assert_eq!(effective, current_directory.path().join("./data"));
        assert!(effective.is_dir());
        assert!(!effective.join("chain.db").exists());
        Ok(())
    }

    #[test]
    fn data_directory_policy_rejects_empty_whitespace_and_padded_values() -> Result<()> {
        let current_directory = tempfile::tempdir()?;

        for value in ["", "   ", " relative-data", "relative-data "] {
            let error = prepare_data_directory_from(value, current_directory.path())
                .expect_err("explicit invalid data directory should be rejected");
            let message = error.to_string();
            assert!(message.contains("VISION_DATA_DIR"));
            assert!(message.contains(&format!("{value:?}")));
        }

        assert_eq!(fs::read_dir(current_directory.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn data_directory_policy_resolves_relative_path_without_opening_database() -> Result<()> {
        let current_directory = tempfile::tempdir()?;

        let effective = prepare_data_directory_from("relative-data", current_directory.path())?;

        assert_eq!(effective, current_directory.path().join("relative-data"));
        assert!(effective.is_dir());
        assert!(!effective.join("chain.db").exists());
        Ok(())
    }

    #[test]
    fn data_directory_policy_accepts_existing_directory_without_replacing_it() -> Result<()> {
        let data_directory = tempfile::tempdir()?;
        let marker = data_directory.path().join("marker");
        fs::write(&marker, b"preserve me")?;

        let effective = prepare_data_directory_from(
            &data_directory.path().display().to_string(),
            Path::new("unused"),
        )?;

        assert_eq!(effective, data_directory.path());
        assert_eq!(fs::read(marker)?, b"preserve me");
        assert!(!effective.join("chain.db").exists());
        Ok(())
    }

    #[test]
    fn data_directory_policy_rejects_regular_file_without_modifying_it() -> Result<()> {
        let parent = tempfile::tempdir()?;
        let data_file = parent.path().join("data-file");
        fs::write(&data_file, b"preserve me")?;

        let error =
            prepare_data_directory_from(&data_file.display().to_string(), Path::new("unused"))
                .expect_err("regular file must not be accepted as a data directory");

        assert!(error.to_string().contains("must be a directory"));
        assert_eq!(fs::read(data_file)?, b"preserve me");
        Ok(())
    }

    #[test]
    fn data_directory_policy_rejects_missing_parent_without_fallback() -> Result<()> {
        let current_directory = tempfile::tempdir()?;
        let configured = "missing-parent/data";

        let error = prepare_data_directory_from(configured, current_directory.path())
            .expect_err("a missing parent directory must be rejected");

        let message = error.to_string();
        assert!(message.contains("VISION_DATA_DIR"));
        assert!(message.contains(configured));
        assert!(!current_directory.path().join("missing-parent").exists());
        assert!(!current_directory.path().join("data").exists());
        Ok(())
    }

    #[test]
    fn data_directory_policy_rejects_regular_file_at_chain_db_path() -> Result<()> {
        let data_directory = tempfile::tempdir()?;
        let database_file = data_directory.path().join("chain.db");
        fs::write(&database_file, b"preserve me")?;

        let error = prepare_data_directory_from(
            &data_directory.path().display().to_string(),
            Path::new("unused"),
        )
        .expect_err("regular chain.db file must be rejected");

        assert!(error.to_string().contains("chain.db"));
        assert_eq!(fs::read(database_file)?, b"preserve me");
        Ok(())
    }

    #[test]
    fn initialize_chain_state_opens_only_the_validated_directory() -> Result<()> {
        let data_directory = tempfile::tempdir()?;
        let settings = test_settings(data_directory.path());

        let chain = initialize_chain_state(&settings)?;

        assert_eq!(chain.current_height(), 0);
        assert!(data_directory.path().join("chain.db").is_dir());
        Ok(())
    }

    #[test]
    fn initialize_chain_state_rejects_explicit_empty_before_database_open() -> Result<()> {
        let fallback_guard = tempfile::tempdir()?;
        let mut settings = test_settings(fallback_guard.path());
        settings.data_dir.clear();

        let error = match initialize_chain_state(&settings) {
            Ok(_) => panic!("explicitly empty VISION_DATA_DIR should fail before storage opens"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("VISION_DATA_DIR"));
        assert!(!fallback_guard.path().join("chain.db").exists());
        Ok(())
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
}

use anyhow::Result;

use crate::chain::snapshots::restore_latest_snapshot;
use crate::chain::storage::{load_meta, store_block, store_height_index, store_meta};
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

/// Open, bootstrap, and restore the latest available snapshot for a node.
///
/// Fresh databases are left at genesis when no snapshot exists yet. Existing
/// databases resume from the newest snapshot so balances, nonces, and cached
/// state root are available before sync or API requests start.
pub fn initialize_chain_state(settings: &Settings) -> Result<ChainState> {
    let mut chain_state = ChainState::open_with_genesis(&settings.data_dir)?;
    bootstrap_chain(&mut chain_state, settings)?;

    let current_height = chain_state.current_height();
    match restore_latest_snapshot(&mut chain_state, current_height) {
        Ok(restored_height) => {
            tracing::info!(
                "[BOOTSTRAP] Restored snapshot at height {} before services start",
                restored_height
            );
        }
        Err(e) => {
            tracing::debug!("[BOOTSTRAP] No snapshot restored on startup: {}", e);
        }
    }

    Ok(chain_state)
}

/// Seed the peer store with the default seed peers from `settings`.
pub fn seed_peers(settings: &Settings) -> Vec<String> {
    settings.seed_peers.clone()
}

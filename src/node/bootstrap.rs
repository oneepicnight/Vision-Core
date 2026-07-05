use anyhow::Result;
use crate::chain::ChainState;
use crate::config::settings::Settings;
use crate::genesis::genesis::{genesis_block, validate_genesis_hash, verify_stored_genesis};
use crate::chain::storage::{load_meta, store_meta, store_block};

/// Initialise the chain from scratch or verify an existing database.
///
/// 1. Validates the compile-time genesis hash constant.
/// 2. If the DB has no genesis block, writes one and returns.
/// 3. If the DB has a genesis block, verifies it matches the canonical hash.
///
/// Aborts on any mismatch — these are irrecoverable without a wipe.
pub fn bootstrap_chain(g: &mut ChainState, _settings: &Settings) -> Result<()> {
    // Step 1: verify our genesis constant is self-consistent.
    validate_genesis_hash()?;

    // Step 2: check whether the database already has a genesis block.
    match load_meta(g, "genesis_hash")? {
        None => {
            // Fresh database — write genesis.
            let genesis = genesis_block();
            store_block(g, &genesis)?;
            store_meta(g, "genesis_hash", genesis.hash())?;
            g.blocks.push(genesis.clone());
            g.canon_index.insert(genesis.hash().to_string(), 0);
            tracing::info!("[BOOTSTRAP] Genesis block written: {}", genesis.hash());
        }
        Some(stored_hash) => {
            // Existing database — verify stored genesis matches canonical.
            verify_stored_genesis(&stored_hash)?;
            tracing::info!("[BOOTSTRAP] Genesis verified: {}", stored_hash);
        }
    }

    Ok(())
}

/// Seed the peer store with the default seed peers from `settings`.
pub fn seed_peers(settings: &Settings) -> Vec<String> {
    settings.seed_peers.clone()
}

use crate::chain::reorg::ReorgRecovery;
use crate::chain::storage::load_meta;
use crate::config::constants::*;
use crate::types::Block;
use sled::Db;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Address type alias for clarity.
pub type Address = String;

/// The complete chain state held in memory.
///
/// This is the single authoritative in-memory representation of the node's
/// blockchain state. There is no secondary in-memory chain representation.
///
/// Access is guarded by a `tokio::sync::Mutex<ChainState>` at the node level.
pub struct ChainState {
    // ─── Canonical chain ──────────────────────────────────────────────────────
    /// All canonical blocks in order, index 0 = genesis.
    /// `blocks[i].header.number == i` is an invariant maintained by all callers.
    pub blocks: Vec<Block>,

    /// Current LWMA-adjusted difficulty for the next block.
    pub difficulty: u64,

    // ─── Account state ────────────────────────────────────────────────────────
    /// Token balances keyed by address (raw units, saturating at u128::MAX).
    pub balances: BTreeMap<Address, u128>,

    /// Per-address nonce counter (last accepted nonce; 0 = no tx yet).
    pub nonces: BTreeMap<Address, u64>,

    // ─── Gossip dedup sets ────────────────────────────────────────────────────
    /// Hashes of transactions already seen (prevents re-relay).
    pub seen_txs: BTreeSet<String>,

    /// Hashes of blocks already seen (prevents re-relay cycles).
    pub seen_blocks: BTreeSet<String>,

    // ─── Fork / side-chain state ──────────────────────────────────────────────
    /// Valid blocks on non-canonical chains.
    /// Key: block.hash(), Value: Block.
    pub side_blocks: BTreeMap<String, Block>,

    /// Cumulative PoW work terminating at each known block hash.
    /// Both canonical and side-chain blocks are tracked here.
    pub cumulative_work: BTreeMap<String, u128>,

    // ─── Orphan pool ──────────────────────────────────────────────────────────
    /// Blocks whose parent is not yet known.
    /// Outer key: expected parent_hash; inner Vec: (block, arrival_secs, peer).
    pub orphan_pool: BTreeMap<String, Vec<(Block, u64, String)>>,

    /// Reverse index: block_hash → parent_hash, used for O(1) eviction.
    pub orphan_by_hash: BTreeMap<String, String>,

    // ─── Indexes ──────────────────────────────────────────────────────────────
    /// block_hash → canonical height; O(1) ancestor resolution.
    pub canon_index: HashMap<String, u64>,

    /// Most-recently verified (height, state_root). Cached so
    /// `apply_block` does not have to recompute it on every call.
    pub cached_state_root: Option<(u64, String)>,

    /// Reorg transaction recovery data produced by the last accepted reorg.
    /// Runtime mempool policy consumes this immediately after block acceptance.
    pub pending_reorg_recovery: Option<ReorgRecovery>,

    // ─── Persistent storage ───────────────────────────────────────────────────
    /// Sled database handle for block and state persistence.
    pub db: Db,
}

impl ChainState {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Open (or create) a chain database at `data_dir/chain.db`.
    ///
    /// Returns an **empty** state; call `open_with_genesis` if you want the
    /// canonical chain re-loaded from the database on startup.
    pub fn open(data_dir: &str) -> anyhow::Result<Self> {
        let db = sled::open(format!("{}/chain.db", data_dir))?;
        Ok(Self::empty(db))
    }

    /// Open the database and reload the stored canonical chain.
    ///
    /// If the database has no chain data yet the function returns an empty
    /// state identical to `open`. The caller should then apply the genesis
    /// block through `chain::accept::apply_block`.
    pub fn open_with_genesis(data_dir: &str) -> anyhow::Result<Self> {
        let db = sled::open(format!("{}/chain.db", data_dir))?;
        let mut state = Self::empty(db);
        let has_persisted_tip = load_meta(&state, "tip_height")?.is_some();

        // Try to recover the canonical chain from persistent storage.
        if let Err(e) = crate::chain::storage::load_canon_chain(&mut state) {
            if has_persisted_tip {
                return Err(e);
            }
            tracing::warn!("[STATE] Could not reload chain from DB: {}", e);
            // Non-fatal: node will re-sync from peers.
        }

        Ok(state)
    }

    /// Construct an empty in-memory state backed by `db`.
    pub(crate) fn empty(db: Db) -> Self {
        Self {
            blocks: Vec::new(),
            difficulty: DIFFICULTY_FLOOR,
            balances: BTreeMap::new(),
            nonces: BTreeMap::new(),
            seen_txs: BTreeSet::new(),
            seen_blocks: BTreeSet::new(),
            side_blocks: BTreeMap::new(),
            cumulative_work: BTreeMap::new(),
            orphan_pool: BTreeMap::new(),
            orphan_by_hash: BTreeMap::new(),
            canon_index: HashMap::new(),
            cached_state_root: None,
            pending_reorg_recovery: None,
            db,
        }
    }

    // ── Chain queries ─────────────────────────────────────────────────────────

    /// Height of the current canonical tip (0 when only genesis exists).
    pub fn current_height(&self) -> u64 {
        self.blocks.last().map(|b| b.header.number).unwrap_or(0)
    }

    /// Hash of the current canonical tip. Returns the null hash when the
    /// chain is empty (before genesis is applied).
    pub fn tip_hash(&self) -> String {
        self.blocks
            .last()
            .map(|b| b.header.pow_hash.clone())
            .unwrap_or_else(|| "0".repeat(64))
    }

    /// Look up a canonical block by height. O(1).
    pub fn block_at(&self, height: u64) -> Option<&Block> {
        self.blocks.get(height as usize)
    }

    /// Look up a block by hash in both the canonical chain and the side-block
    /// store. Returns a cloned `Block` so the caller owns it independently.
    pub fn block_by_hash(&self, hash: &str) -> Option<Block> {
        if let Some(&h) = self.canon_index.get(hash) {
            return self.blocks.get(h as usize).cloned();
        }
        self.side_blocks.get(hash).cloned()
    }

    // ── Account state mutation ────────────────────────────────────────────────

    /// Credit `amount` to `address`, saturating at `u128::MAX`.
    pub fn credit_balance(&mut self, address: &str, amount: u128) {
        let bal = self.balances.entry(address.to_string()).or_insert(0);
        *bal = bal.saturating_add(amount);
    }

    /// Debit `amount` from `address`.
    ///
    /// Returns `Err` if the address has insufficient funds; the balance is
    /// left unchanged on failure.
    pub fn debit_balance(&mut self, address: &str, amount: u128) -> anyhow::Result<()> {
        let bal = self.balances.get(address).copied().unwrap_or(0);
        if bal < amount {
            return Err(anyhow::anyhow!(
                "insufficient balance for {}: have {} need {}",
                address,
                bal,
                amount
            ));
        }
        self.balances.insert(address.to_string(), bal - amount);
        Ok(())
    }

    /// Advance the nonce for `address` to `next_nonce`.
    ///
    /// Only moves forward; a `next_nonce` that is ≤ the stored value is a
    /// no-op (prevents replay attacks from being silently accepted).
    pub fn advance_nonce(&mut self, address: &str, next_nonce: u64) {
        let entry = self.nonces.entry(address.to_string()).or_insert(0);
        if next_nonce > *entry {
            *entry = next_nonce;
        }
    }

    /// Return the current nonce for `address` (0 if no tx has been accepted).
    pub fn nonce_of(&self, address: &str) -> u64 {
        self.nonces.get(address).copied().unwrap_or(0)
    }

    /// Return the current balance for `address` (0 if unknown).
    pub fn balance_of(&self, address: &str) -> u128 {
        self.balances.get(address).copied().unwrap_or(0)
    }

    /// Refresh the cached state root from the current canonical tip.
    pub fn refresh_cached_state_root_from_tip(&mut self) {
        self.cached_state_root = self
            .blocks
            .last()
            .map(|tip| (tip.header.number, tip.header.state_root.clone()));
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::apply_block;
    use crate::genesis::genesis_block;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    #[test]
    fn empty_state_has_no_blocks() {
        let g = temp_state();
        assert!(g.blocks.is_empty());
        assert_eq!(g.current_height(), 0);
    }

    #[test]
    fn tip_hash_before_genesis_is_null() {
        let g = temp_state();
        assert_eq!(g.tip_hash(), "0".repeat(64));
    }

    #[test]
    fn block_at_returns_correct_block() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        assert_eq!(g.block_at(0).unwrap().hash(), gen.hash());
    }

    #[test]
    fn block_by_hash_finds_canonical() {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let found = g.block_by_hash(gen.hash());
        assert!(found.is_some());
        assert_eq!(found.unwrap().hash(), gen.hash());
    }

    #[test]
    fn block_by_hash_finds_side_block() {
        use crate::config::constants::TARGET_BLOCK_TIME;
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);

        // Build a canonical b1 and a competing b1_prime.
        let b1 = crate::chain::accept::tests_helpers::make_test_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        let b1p = crate::chain::accept::tests_helpers::make_test_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAB,
        );
        apply_block(&mut g, &b1, None);
        apply_block(&mut g, &b1p, None);

        // b1p must be reachable via block_by_hash even though it's a side block.
        assert!(g.block_by_hash(b1p.hash()).is_some());
    }

    #[test]
    fn credit_increases_balance() {
        let mut g = temp_state();
        g.credit_balance("alice", 1_000);
        assert_eq!(g.balance_of("alice"), 1_000);
        g.credit_balance("alice", 500);
        assert_eq!(g.balance_of("alice"), 1_500);
    }

    #[test]
    fn debit_decreases_balance() {
        let mut g = temp_state();
        g.credit_balance("alice", 1_000);
        g.debit_balance("alice", 400).unwrap();
        assert_eq!(g.balance_of("alice"), 600);
    }

    #[test]
    fn debit_insufficient_funds_returns_err() {
        let mut g = temp_state();
        g.credit_balance("alice", 10);
        let result = g.debit_balance("alice", 100);
        assert!(result.is_err());
        // Balance unchanged.
        assert_eq!(g.balance_of("alice"), 10);
    }

    #[test]
    fn advance_nonce_only_moves_forward() {
        let mut g = temp_state();
        g.advance_nonce("bob", 5);
        assert_eq!(g.nonce_of("bob"), 5);
        g.advance_nonce("bob", 3); // backwards — should be ignored
        assert_eq!(g.nonce_of("bob"), 5);
        g.advance_nonce("bob", 7);
        assert_eq!(g.nonce_of("bob"), 7);
    }

    #[test]
    fn balance_of_unknown_address_is_zero() {
        let g = temp_state();
        assert_eq!(g.balance_of("nobody"), 0);
    }

    #[test]
    fn nonce_of_unknown_address_is_zero() {
        let g = temp_state();
        assert_eq!(g.nonce_of("nobody"), 0);
    }
    #[test]
    fn refresh_cached_state_root_from_tip_tracks_canonical_tip() {
        use crate::config::constants::TARGET_BLOCK_TIME;

        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        let b1 = crate::chain::accept::tests_helpers::make_test_block(
            gen.hash(),
            1,
            gen.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
        );
        apply_block(&mut g, &b1, None);

        g.cached_state_root = None;
        g.refresh_cached_state_root_from_tip();

        assert_eq!(g.cached_state_root, Some((1, b1.header.state_root.clone())));
    }
}

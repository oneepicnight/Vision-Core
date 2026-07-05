use std::collections::{HashMap, VecDeque};
use crate::config::constants::MEMPOOL_MAX;
use crate::types::Tx;

/// Inner pool state, accessed exclusively through `Mempool`.
struct Pool {
    /// Primary index: tx_id → Transaction.
    by_id: HashMap<String, Tx>,

    /// Insertion-ordered queue of tx_ids (FIFO eviction when full).
    order: VecDeque<String>,
}

impl Pool {
    fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

/// Thread-safe transaction mempool.
///
/// Maintains a bounded set of pending transactions ordered by insertion time.
/// When the pool is full, the oldest transaction is evicted to make room.
pub struct Mempool {
    inner: std::sync::Arc<std::sync::Mutex<Pool>>,
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(Pool::new())),
        }
    }

    /// Insert a transaction. Returns `false` if the tx was already present.
    pub fn insert(&self, tx: Tx) -> bool {
        let id = tx.tx_id();
        let mut p = self.inner.lock().unwrap();
        if p.by_id.contains_key(&id) {
            return false;
        }
        // Evict oldest if at capacity.
        if p.by_id.len() >= MEMPOOL_MAX {
            if let Some(evict_id) = p.order.pop_front() {
                p.by_id.remove(&evict_id);
            }
        }
        p.order.push_back(id.clone());
        p.by_id.insert(id, tx);
        true
    }

    /// Check whether a tx_id is present.
    pub fn has(&self, tx_id: &str) -> bool {
        self.inner.lock().unwrap().by_id.contains_key(tx_id)
    }

    /// Retrieve a transaction by id.
    pub fn get(&self, tx_id: &str) -> Option<Tx> {
        self.inner.lock().unwrap().by_id.get(tx_id).cloned()
    }

    /// Number of transactions currently in the pool.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return all tx_ids in insertion order.
    pub fn list_ids(&self) -> Vec<String> {
        self.inner.lock().unwrap().order.iter().cloned().collect()
    }

    /// Select up to `limit` transactions for inclusion in the next block.
    ///
    /// Returns transactions in insertion order (oldest first).
    pub fn select_for_block(&self, limit: usize) -> Vec<Tx> {
        let p = self.inner.lock().unwrap();
        p.order
            .iter()
            .take(limit)
            .filter_map(|id| p.by_id.get(id).cloned())
            .collect()
    }

    /// Remove a set of transaction ids (called after block acceptance).
    pub fn remove_confirmed(&self, tx_ids: &[String]) {
        let mut p = self.inner.lock().unwrap();
        for id in tx_ids {
            p.by_id.remove(id);
        }
        // Collect owned keys to release the immutable borrow before calling
        // retain (which needs a mutable borrow of p.order).
        let keep: std::collections::HashSet<String> =
            p.by_id.keys().cloned().collect();
        p.order.retain(|id| keep.contains(id));
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal non-coinbase transaction with a given nonce so that
    /// different nonces produce different tx_ids.
    fn make_tx(nonce: u64) -> Tx {
        Tx {
            nonce,
            sender_pubkey: "aa".repeat(32),
            module:        "transfer".to_string(),
            method:        "send".to_string(),
            args:          vec![],
            tip:           0,
            fee_limit:     0,
            sig:           "sig".to_string(),
        }
    }

    // ── insert / duplicate rejection ─────────────────────────────────────────

    #[test]
    fn insert_returns_true_for_new_tx() {
        let mp = Mempool::new();
        assert!(mp.insert(make_tx(1)));
    }

    #[test]
    fn insert_returns_false_for_duplicate() {
        let mp = Mempool::new();
        let tx = make_tx(42);
        assert!(mp.insert(tx.clone()));
        assert!(!mp.insert(tx));
    }

    #[test]
    fn duplicate_does_not_increase_len() {
        let mp = Mempool::new();
        let tx = make_tx(7);
        mp.insert(tx.clone());
        mp.insert(tx);
        assert_eq!(mp.len(), 1);
    }

    // ── has / get ─────────────────────────────────────────────────────────────

    #[test]
    fn has_returns_true_after_insert() {
        let mp = Mempool::new();
        let tx = make_tx(1);
        let id = tx.tx_id();
        mp.insert(tx);
        assert!(mp.has(&id));
    }

    #[test]
    fn has_returns_false_for_unknown_id() {
        let mp = Mempool::new();
        assert!(!mp.has("deadbeef"));
    }

    #[test]
    fn get_returns_tx_by_id() {
        let mp = Mempool::new();
        let tx = make_tx(3);
        let id = tx.tx_id();
        mp.insert(tx.clone());
        assert_eq!(mp.get(&id), Some(tx));
    }

    #[test]
    fn get_returns_none_for_missing_id() {
        let mp = Mempool::new();
        assert!(mp.get("missing").is_none());
    }

    // ── list_ids / select_for_block ───────────────────────────────────────────

    #[test]
    fn list_ids_preserves_insertion_order() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let t3 = make_tx(3);
        let id1 = t1.tx_id();
        let id2 = t2.tx_id();
        let id3 = t3.tx_id();
        mp.insert(t1);
        mp.insert(t2);
        mp.insert(t3);
        assert_eq!(mp.list_ids(), vec![id1, id2, id3]);
    }

    #[test]
    fn select_for_block_respects_limit() {
        let mp = Mempool::new();
        for n in 0..10 {
            mp.insert(make_tx(n));
        }
        assert_eq!(mp.select_for_block(4).len(), 4);
    }

    #[test]
    fn select_for_block_returns_oldest_first() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let id1 = t1.tx_id();
        mp.insert(t1);
        mp.insert(t2);
        let selected = mp.select_for_block(1);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].tx_id(), id1);
    }

    // ── remove_confirmed (block-application removal) ──────────────────────────

    #[test]
    fn remove_confirmed_drops_included_txs() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let t3 = make_tx(3);
        let id1 = t1.tx_id();
        let id2 = t2.tx_id();
        mp.insert(t1);
        mp.insert(t2);
        mp.insert(t3.clone());

        mp.remove_confirmed(&[id1.clone(), id2.clone()]);

        assert!(!mp.has(&id1));
        assert!(!mp.has(&id2));
        assert!(mp.has(&t3.tx_id()));
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn remove_confirmed_keeps_order_consistent() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let t3 = make_tx(3);
        let id1 = t1.tx_id();
        let id3 = t3.tx_id();
        mp.insert(t1);
        mp.insert(t2.clone());
        mp.insert(t3);

        mp.remove_confirmed(&[t2.tx_id()]);

        // Only t1 and t3 should remain, in original order.
        assert_eq!(mp.list_ids(), vec![id1, id3]);
    }

    #[test]
    fn remove_confirmed_unknown_ids_are_ignored() {
        let mp = Mempool::new();
        mp.insert(make_tx(1));
        // Should not panic on unknown ids.
        mp.remove_confirmed(&["no_such_id".to_string()]);
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn remove_confirmed_all_leaves_empty_pool() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let id1 = t1.tx_id();
        let id2 = t2.tx_id();
        mp.insert(t1);
        mp.insert(t2);

        mp.remove_confirmed(&[id1, id2]);

        assert!(mp.is_empty());
        assert_eq!(mp.list_ids(), Vec::<String>::new());
    }

    // ── capacity / eviction ───────────────────────────────────────────────────

    #[test]
    fn pool_never_exceeds_mempool_max() {
        let mp = Mempool::new();
        for n in 0..=(MEMPOOL_MAX as u64 + 10) {
            mp.insert(make_tx(n));
        }
        assert!(mp.len() <= MEMPOOL_MAX, "len {} exceeds MEMPOOL_MAX {}", mp.len(), MEMPOOL_MAX);
    }

    #[test]
    fn eviction_removes_oldest_when_full() {
        let mp = Mempool::new();
        // Fill to capacity.
        for n in 0..MEMPOOL_MAX as u64 {
            mp.insert(make_tx(n));
        }
        let first_id = make_tx(0).tx_id();
        assert!(mp.has(&first_id), "sanity: first tx present before overflow");

        // One more evicts the oldest.
        mp.insert(make_tx(MEMPOOL_MAX as u64));
        assert!(!mp.has(&first_id), "oldest tx should have been evicted");
    }
}

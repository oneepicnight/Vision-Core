use std::collections::{HashMap, HashSet, VecDeque};

use super::admission::{AdmissionDecision, MempoolAdmission, MempoolAdmissionError};
use crate::chain::reorg::ReorgRecovery;
use crate::chain::ChainState;
use crate::config::constants::MEMPOOL_MAX;
use crate::types::transaction::canonical_tx_id;
use crate::types::Tx;

/// Inner pool state, accessed exclusively through `Mempool`.
struct Pool {
    /// Primary index: canonical tx_id -> transaction.
    by_id: HashMap<String, Tx>,

    /// Insertion-ordered queue of canonical tx_ids.
    order: VecDeque<String>,
}

fn tx_key(tx: &Tx) -> String {
    canonical_tx_id(tx)
}

fn insert_locked(pool: &mut Pool, id: String, tx: Tx) {
    if pool.by_id.len() >= MEMPOOL_MAX {
        if let Some(evict_id) = pool.order.pop_front() {
            pool.by_id.remove(&evict_id);
        }
    }

    pool.order.push_back(id.clone());
    pool.by_id.insert(id, tx);
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReorgRequeueReport {
    pub accepted: Vec<String>,
    pub rejected: Vec<(String, MempoolAdmissionError)>,
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

    /// Insert a transaction without running admission checks.
    ///
    /// This keeps the legacy FIFO helper available for tests and internal setup,
    /// but keys the pool by canonical transaction id.
    pub fn insert(&self, tx: Tx) -> bool {
        let id = tx_key(&tx);
        let mut p = self.inner.lock().unwrap();
        if p.by_id.contains_key(&id) {
            return false;
        }

        insert_locked(&mut p, id, tx);
        true
    }

    /// Validate and admit a transaction against the canonical mempool policy.
    pub fn admit(
        &self,
        tx: Tx,
        current_nonce: u64,
    ) -> Result<AdmissionDecision, MempoolAdmissionError> {
        let mut p = self.inner.lock().unwrap();
        let pending: Vec<Tx> = p
            .order
            .iter()
            .filter_map(|id| p.by_id.get(id).cloned())
            .collect();
        let decision = MempoolAdmission::new(current_nonce, &pending).evaluate(&tx)?;

        match &decision {
            AdmissionDecision::Accept => {
                insert_locked(&mut p, tx_key(&tx), tx);
            }
            AdmissionDecision::Replace { evict_tx_id } => {
                p.by_id.remove(evict_tx_id);
                p.order.retain(|id| id != evict_tx_id);
                insert_locked(&mut p, tx_key(&tx), tx);
            }
        }

        Ok(decision)
    }

    /// Check whether a canonical tx_id is present.
    pub fn has(&self, tx_id: &str) -> bool {
        self.inner.lock().unwrap().by_id.contains_key(tx_id)
    }

    /// Retrieve a transaction by canonical tx_id.
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

    /// Return all canonical tx_ids in insertion order.
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

    /// Remove a set of canonical transaction ids after block acceptance.
    pub fn remove_confirmed(&self, tx_ids: &[String]) {
        let mut p = self.inner.lock().unwrap();
        for id in tx_ids {
            p.by_id.remove(id);
        }
        let keep: HashSet<String> = p.by_id.keys().cloned().collect();
        p.order.retain(|id| keep.contains(id));
    }

    /// Re-admit eligible non-coinbase transactions displaced by a successful reorg.
    pub fn requeue_after_reorg(
        &self,
        chain: &ChainState,
        recovery: ReorgRecovery,
    ) -> ReorgRequeueReport {
        let winning: HashSet<String> = recovery.winning_tx_ids.iter().cloned().collect();
        self.remove_confirmed(&recovery.winning_tx_ids);

        let mut candidates: Vec<Tx> = recovery
            .displaced_txs
            .into_iter()
            .filter(|tx| tx.module != "coinbase")
            .filter(|tx| !winning.contains(&canonical_tx_id(tx)))
            .collect();
        candidates.sort_by(|a, b| {
            a.sender_pubkey
                .cmp(&b.sender_pubkey)
                .then(a.nonce.cmp(&b.nonce))
        });

        let mut report = ReorgRequeueReport::default();
        for tx in candidates {
            let tx_id = canonical_tx_id(&tx);
            let current_nonce = chain.nonce_of(&tx.sender_pubkey);
            match self.admit(tx, current_nonce) {
                Ok(_) => report.accepted.push(tx_id),
                Err(err) => {
                    tracing::debug!(
                        "[MEMPOOL] displaced tx {} not requeued after reorg: {:?}",
                        tx_id,
                        err
                    );
                    report.rejected.push((tx_id, err));
                }
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::transaction::{
        canonical_unsigned_payload, CashTransferArgs, TxValidationError,
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn make_tx(nonce: u64) -> Tx {
        Tx {
            nonce,
            sender_pubkey: "aa".repeat(32),
            module: "transfer".to_string(),
            method: "send".to_string(),
            args: vec![],
            tip: 0,
            fee_limit: 0,
            sig: "sig".to_string(),
        }
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn sign_tx(mut tx: Tx, seed: u8) -> Tx {
        let signing_key = signing_key(seed);
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();
        let sig = signing_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    fn signed_transfer_tx(seed: u8, nonce: u64, tip: u64, fee_limit: u64, amount: u128) -> Tx {
        sign_tx(
            Tx {
                nonce,
                sender_pubkey: String::new(),
                module: "cash".to_string(),
                method: "transfer".to_string(),
                args: transfer_args(&"22".repeat(32), amount),
                tip,
                fee_limit,
                sig: String::new(),
            },
            seed,
        )
    }

    fn temp_chain_with_nonce(sender: &str, nonce: u64) -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let mut chain = ChainState::empty(db);
        if nonce > 0 {
            chain.nonces.insert(sender.to_string(), nonce);
        }
        chain
    }

    fn reorg_recovery(displaced_txs: Vec<Tx>, winning_tx_ids: Vec<String>) -> ReorgRecovery {
        ReorgRecovery {
            displaced_txs,
            winning_tx_ids,
        }
    }

    #[test]
    fn displaced_valid_transaction_returns_to_mempool() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1_000, 1);
        let tx_id = canonical_tx_id(&tx);
        let chain = temp_chain_with_nonce(&tx.sender_pubkey, 0);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![tx], vec![]));

        assert_eq!(report.accepted, vec![tx_id.clone()]);
        assert!(report.rejected.is_empty());
        assert!(mp.has(&tx_id));
    }

    #[test]
    fn transaction_on_winning_branch_is_not_requeued() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1_000, 1);
        let tx_id = canonical_tx_id(&tx);
        let chain = temp_chain_with_nonce(&tx.sender_pubkey, 0);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![tx], vec![tx_id]));

        assert!(report.accepted.is_empty());
        assert!(report.rejected.is_empty());
        assert!(mp.is_empty());
    }

    #[test]
    fn stale_displaced_transaction_is_rejected() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1_000, 1);
        let chain = temp_chain_with_nonce(&tx.sender_pubkey, 2);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![tx], vec![]));

        assert!(report.accepted.is_empty());
        assert!(matches!(
            report.rejected[0].1,
            MempoolAdmissionError::StaleNonce { current_nonce: 2, tx_nonce: 0 }
        ));
        assert!(mp.is_empty());
    }

    #[test]
    fn invalid_signature_displaced_transaction_is_rejected() {
        let mp = Mempool::new();
        let mut tx = signed_transfer_tx(1, 0, 2, 1_000, 1);
        tx.sig = "00".repeat(64);
        let chain = temp_chain_with_nonce(&tx.sender_pubkey, 0);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![tx], vec![]));

        assert!(report.accepted.is_empty());
        assert!(matches!(
            report.rejected[0].1,
            MempoolAdmissionError::StatelessValidation(_)
        ));
        assert!(mp.is_empty());
    }

    #[test]
    fn multiple_displaced_transactions_preserve_nonce_order() {
        let mp = Mempool::new();
        let tx0 = signed_transfer_tx(1, 0, 2, 1_000, 1);
        let tx1 = signed_transfer_tx(1, 1, 2, 1_000, 1);
        let id0 = canonical_tx_id(&tx0);
        let id1 = canonical_tx_id(&tx1);
        let chain = temp_chain_with_nonce(&tx0.sender_pubkey, 0);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![tx1, tx0], vec![]));

        assert_eq!(report.accepted, vec![id0.clone(), id1.clone()]);
        assert_eq!(mp.list_ids(), vec![id0, id1]);
    }

    #[test]
    fn coinbase_transactions_are_never_requeued() {
        let mp = Mempool::new();
        let coinbase = Tx {
            nonce: 0,
            sender_pubkey: String::new(),
            module: "coinbase".to_string(),
            method: "reward".to_string(),
            args: 1u64.to_be_bytes().to_vec(),
            tip: 0,
            fee_limit: 0,
            sig: String::new(),
        };
        let chain = temp_chain_with_nonce("", 0);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![coinbase], vec![]));

        assert!(report.accepted.is_empty());
        assert!(report.rejected.is_empty());
        assert!(mp.is_empty());
    }

    #[test]
    fn conflicting_sender_nonce_policy_remains_enforced_during_requeue() {
        let mp = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 5, 1_000, 1);
        let displaced = signed_transfer_tx(1, 0, 2, 1_000, 2);
        assert!(mp.insert(existing));
        let chain = temp_chain_with_nonce(&displaced.sender_pubkey, 0);

        let report = mp.requeue_after_reorg(&chain, reorg_recovery(vec![displaced], vec![]));

        assert!(report.accepted.is_empty());
        assert!(matches!(
            report.rejected[0].1,
            MempoolAdmissionError::DuplicateSenderNonce { .. }
        ));
        assert_eq!(mp.len(), 1);
    }

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
    fn insert_rejects_duplicate_canonical_tx_id_when_only_sig_changes() {
        let mp = Mempool::new();
        let tx = make_tx(42);
        let mut same_unsigned = tx.clone();
        same_unsigned.sig = "different-signature".to_string();

        assert!(mp.insert(tx));
        assert!(!mp.insert(same_unsigned));
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn duplicate_does_not_increase_len() {
        let mp = Mempool::new();
        let tx = make_tx(7);
        mp.insert(tx.clone());
        mp.insert(tx);
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn has_returns_true_after_insert() {
        let mp = Mempool::new();
        let tx = make_tx(1);
        let id = canonical_tx_id(&tx);
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
        let id = canonical_tx_id(&tx);
        mp.insert(tx.clone());
        assert_eq!(mp.get(&id), Some(tx));
    }

    #[test]
    fn get_returns_none_for_missing_id() {
        let mp = Mempool::new();
        assert!(mp.get("missing").is_none());
    }

    #[test]
    fn list_ids_preserves_insertion_order() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let t3 = make_tx(3);
        let id1 = canonical_tx_id(&t1);
        let id2 = canonical_tx_id(&t2);
        let id3 = canonical_tx_id(&t3);
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
        let id1 = canonical_tx_id(&t1);
        mp.insert(t1);
        mp.insert(t2);
        let selected = mp.select_for_block(1);
        assert_eq!(selected.len(), 1);
        assert_eq!(canonical_tx_id(&selected[0]), id1);
    }

    #[test]
    fn remove_confirmed_drops_included_txs() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let t3 = make_tx(3);
        let id1 = canonical_tx_id(&t1);
        let id2 = canonical_tx_id(&t2);
        let id3 = canonical_tx_id(&t3);
        mp.insert(t1);
        mp.insert(t2);
        mp.insert(t3);

        mp.remove_confirmed(&[id1.clone(), id2.clone()]);

        assert!(!mp.has(&id1));
        assert!(!mp.has(&id2));
        assert!(mp.has(&id3));
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn remove_confirmed_keeps_order_consistent() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let t3 = make_tx(3);
        let id1 = canonical_tx_id(&t1);
        let id2 = canonical_tx_id(&t2);
        let id3 = canonical_tx_id(&t3);
        mp.insert(t1);
        mp.insert(t2);
        mp.insert(t3);

        mp.remove_confirmed(&[id2]);

        assert_eq!(mp.list_ids(), vec![id1, id3]);
    }

    #[test]
    fn remove_confirmed_unknown_ids_are_ignored() {
        let mp = Mempool::new();
        mp.insert(make_tx(1));
        mp.remove_confirmed(&["no_such_id".to_string()]);
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn remove_confirmed_all_leaves_empty_pool() {
        let mp = Mempool::new();
        let t1 = make_tx(1);
        let t2 = make_tx(2);
        let id1 = canonical_tx_id(&t1);
        let id2 = canonical_tx_id(&t2);
        mp.insert(t1);
        mp.insert(t2);

        mp.remove_confirmed(&[id1, id2]);

        assert!(mp.is_empty());
        assert_eq!(mp.list_ids(), Vec::<String>::new());
    }

    #[test]
    fn pool_never_exceeds_mempool_max() {
        let mp = Mempool::new();
        for n in 0..=(MEMPOOL_MAX as u64 + 10) {
            mp.insert(make_tx(n));
        }
        assert!(
            mp.len() <= MEMPOOL_MAX,
            "len {} exceeds MEMPOOL_MAX {}",
            mp.len(),
            MEMPOOL_MAX
        );
    }

    #[test]
    fn eviction_removes_oldest_when_full() {
        let mp = Mempool::new();
        for n in 0..MEMPOOL_MAX as u64 {
            mp.insert(make_tx(n));
        }
        let first_id = canonical_tx_id(&make_tx(0));
        assert!(mp.has(&first_id), "sanity: first tx present before overflow");

        mp.insert(make_tx(MEMPOOL_MAX as u64));
        assert!(!mp.has(&first_id), "oldest tx should have been evicted");
    }

    #[test]
    fn admit_accepts_valid_tx_and_stores_by_canonical_id() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1_000, 1);
        let id = canonical_tx_id(&tx);

        assert_eq!(mp.admit(tx.clone(), 0), Ok(AdmissionDecision::Accept));
        assert!(mp.has(&id));
        assert_eq!(mp.get(&id), Some(tx));
    }

    #[test]
    fn admit_rejects_invalid_signature_without_insert() {
        let mp = Mempool::new();
        let mut tx = signed_transfer_tx(1, 0, 2, 1_000, 1);
        tx.sig = "00".repeat(64);

        assert!(matches!(
            mp.admit(tx, 0),
            Err(MempoolAdmissionError::StatelessValidation(
                TxValidationError::Signature(_)
            ))
        ));
        assert!(mp.is_empty());
    }

    #[test]
    fn admit_rejects_fee_limit_below_threshold_without_insert() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 200, 1);

        assert_eq!(
            mp.admit(tx, 0),
            Err(MempoolAdmissionError::StatelessValidation(
                TxValidationError::FeeLimitTooLow
            ))
        );
        assert!(mp.is_empty());
    }

    #[test]
    fn admit_rejects_stale_nonce() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 4, 2, 1_000, 1);

        assert_eq!(
            mp.admit(tx, 5),
            Err(MempoolAdmissionError::StaleNonce {
                current_nonce: 5,
                tx_nonce: 4,
            })
        );
    }

    #[test]
    fn admit_rejects_nonce_gap_greater_than_one() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 7, 2, 1_000, 1);

        assert_eq!(
            mp.admit(tx, 5),
            Err(MempoolAdmissionError::NonceGap {
                current_nonce: 5,
                tx_nonce: 7,
            })
        );
    }

    #[test]
    fn admit_rejects_duplicate_canonical_tx_id() {
        let mp = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1_000, 1);

        assert_eq!(mp.admit(tx.clone(), 0), Ok(AdmissionDecision::Accept));
        assert_eq!(
            mp.admit(tx, 0),
            Err(MempoolAdmissionError::DuplicateCanonicalTxId)
        );
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn admit_rejects_duplicate_sender_nonce_without_higher_tip() {
        let mp = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 3, 1_000, 1);
        let replacement = signed_transfer_tx(1, 0, 3, 1_000, 2);

        assert_eq!(mp.admit(existing, 0), Ok(AdmissionDecision::Accept));
        assert_eq!(
            mp.admit(replacement.clone(), 0),
            Err(MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: replacement.sender_pubkey.clone(),
                nonce: 0,
                existing_tip: 3,
                new_tip: 3,
            })
        );
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn admit_replaces_same_sender_nonce_with_strictly_higher_tip() {
        let mp = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 3, 1_000, 1);
        let replacement = signed_transfer_tx(1, 0, 4, 1_000, 1);
        let old_id = canonical_tx_id(&existing);
        let new_id = canonical_tx_id(&replacement);

        assert_eq!(mp.admit(existing, 0), Ok(AdmissionDecision::Accept));
        assert_eq!(
            mp.admit(replacement.clone(), 0),
            Ok(AdmissionDecision::Replace {
                evict_tx_id: old_id.clone(),
            })
        );

        assert_eq!(mp.len(), 1);
        assert!(!mp.has(&old_id));
        assert!(mp.has(&new_id));
        assert_eq!(mp.get(&new_id), Some(replacement));
    }

    #[test]
    fn admit_rejects_replacement_with_lower_tip_without_mutation() {
        let mp = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 4, 1_000, 1);
        let replacement = signed_transfer_tx(1, 0, 3, 1_000, 2);
        let old_id = canonical_tx_id(&existing);
        let new_id = canonical_tx_id(&replacement);

        assert_eq!(mp.admit(existing, 0), Ok(AdmissionDecision::Accept));
        assert_eq!(
            mp.admit(replacement.clone(), 0),
            Err(MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: replacement.sender_pubkey.clone(),
                nonce: 0,
                existing_tip: 4,
                new_tip: 3,
            })
        );

        assert_eq!(mp.len(), 1);
        assert!(mp.has(&old_id));
        assert!(!mp.has(&new_id));
    }
}

use crate::config::constants::{EMISSION_PER_BLOCK, HALVING_INTERVAL, BLOCK_TARGET_TXS};
use crate::pow::difficulty::{calculate_next_difficulty, verify_pow_hash};
use crate::types::{Block, BlockHeader, Tx};

/// A mining job dispatched to worker threads.
///
/// Workers iterate `nonce` values and test whether the resulting VisionX
/// hash meets `target_difficulty`. The first worker to find a valid nonce
/// should call `MinerManager::submit_solution`.
#[derive(Debug, Clone)]
pub struct MiningJob {
    /// Partially-filled block header used as the PoW input template.
    /// `nonce` and `pow_hash` are filled in by the worker.
    pub header_template: BlockHeader,

    /// Difficulty the found hash must satisfy (hash ≤ target(difficulty)).
    pub target_difficulty: u64,

    /// VisionX epoch number derived from the parent block height.
    /// Used to select (or build) the correct dataset.
    pub epoch: u64,

    /// The canonical serialised bytes of the header template fed to VisionX.
    pub header_bytes: Vec<u8>,

    /// Monotonic job id so workers can detect stale jobs without cloning the
    /// full header.
    pub job_id: u64,

    /// Transactions (coinbase first) to include in the candidate block.
    pub txs: Vec<Tx>,
}

impl MiningJob {
    /// Serialise the header template into the canonical byte layout used by
    /// the VisionX hash function.
    ///
    /// Delegates to `BlockHeader::canonical_bytes` to guarantee a single
    /// encoding definition shared with verification.
    pub fn encode_header(h: &BlockHeader) -> Vec<u8> {
        h.canonical_bytes()
    }

    /// Try a single nonce value.  Computes `block.hash()` (blake3 of the
    /// header canonical bytes with the nonce set) and checks PoW.
    ///
    /// Returns `Some(Block)` the moment a valid nonce is found, `None` if
    /// this nonce does not satisfy the difficulty target.
    pub fn try_nonce(&self, nonce: u64) -> Option<Block> {
        let mut header = self.header_template.clone();
        header.nonce = nonce;
        // pow_hash is set to empty string in the template; compute_hash uses
        // canonical_bytes which does NOT include pow_hash, so we first compute
        // the block hash with an empty pow_hash, then store it as pow_hash.
        header.pow_hash = String::new();
        let candidate_hash = header.compute_hash();
        if verify_pow_hash(&candidate_hash, self.target_difficulty) {
            header.pow_hash = candidate_hash;
            let blk = Block {
                header,
                txs: self.txs.clone(),
                weight: self.txs.len() as u64,
            };
            Some(blk)
        } else {
            None
        }
    }
}

// ─── Public helpers ────────────────────────────────────────────────────────────

/// Block subsidy at `height`.  Halves every `HALVING_INTERVAL` blocks.
pub fn block_reward(height: u64) -> u128 {
    EMISSION_PER_BLOCK >> (height / HALVING_INTERVAL)
}

/// Build a `MiningJob` on top of `tip`.
///
/// `job_id`          — monotonic id assigned by the caller (from `MinerManager::next_job_id()`).
/// `miner_addr`      — address credited with the block reward.
/// `extra_txs`       — additional transactions drawn from the mempool (without coinbase).
/// `ancestor_window` — ancestor blocks collected by `collect_ancestor_window`; passed in so
///                     the caller controls how many ancestors to load.
/// `now_secs`        — current unix timestamp for the candidate header.
pub fn build_candidate(
    tip: &Block,
    job_id: u64,
    miner_addr: &str,
    mut extra_txs: Vec<Tx>,
    ancestor_window: &[Block],
    now_secs: u64,
) -> MiningJob {
    let height = tip.header.number + 1;

    // Coinbase must be transaction 0.
    let coinbase = Tx {
        nonce:         height,
        sender_pubkey: String::new(),
        module:        "coinbase".to_string(),
        method:        "reward".to_string(),
        args:          height.to_be_bytes().to_vec(),
        tip:           0,
        fee_limit:     0,
        sig:           String::new(),
    };

    // Cap transactions at the target block size (coinbase + up to BLOCK_TARGET_TXS - 1 others).
    extra_txs.truncate(BLOCK_TARGET_TXS.saturating_sub(1));
    let mut txs = Vec::with_capacity(extra_txs.len() + 1);
    txs.push(coinbase);
    txs.extend(extra_txs);

    // Compute the transaction Merkle root.
    let tx_root = {
        let mut h = blake3::Hasher::new();
        for tx in &txs {
            h.update(tx.tx_id().as_bytes());
        }
        hex::encode(h.finalize().as_bytes())
    };

    let difficulty = calculate_next_difficulty(ancestor_window, now_secs);

    let header_template = BlockHeader {
        parent_hash: tip.hash().to_string(),
        number:      height,
        timestamp:   now_secs,
        difficulty,
        nonce:       0,
        pow_hash:    String::new(),
        state_root:  "0".repeat(64),
        tx_root,
        miner:       miner_addr.to_string(),
    };

    let header_bytes = MiningJob::encode_header(&header_template);
    let epoch = crate::pow::visionx::VISIONX_PARAMS.epoch(height);

    MiningJob {
        header_template,
        target_difficulty: difficulty,
        epoch,
        header_bytes,
        job_id,
        txs,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::constants::{DIFFICULTY_FLOOR, HALVING_INTERVAL, TARGET_BLOCK_TIME};
    use crate::chain::accept::tests_helpers::make_test_block;
    use crate::genesis::genesis_block;

    // Helper: genesis tip as a well-formed Block.
    fn genesis_tip() -> Block {
        genesis_block()
    }

    fn simple_window(tip: &Block) -> Vec<Block> {
        vec![tip.clone()]
    }

    #[test]
    fn block_reward_at_genesis() {
        assert_eq!(block_reward(0), EMISSION_PER_BLOCK);
    }

    #[test]
    fn block_reward_halves_at_interval() {
        assert_eq!(block_reward(HALVING_INTERVAL), EMISSION_PER_BLOCK / 2);
        assert_eq!(block_reward(HALVING_INTERVAL * 2), EMISSION_PER_BLOCK / 4);
    }

    #[test]
    fn block_reward_never_zero_before_64_halvings() {
        // EMISSION_PER_BLOCK is a 40-bit value, so the reward reaches zero
        // after ~40 halvings. Verify every halving before that is > 0.
        let halvings_until_zero = (128 - EMISSION_PER_BLOCK.leading_zeros()) as u64;
        for h in 0..halvings_until_zero {
            assert!(block_reward(h * HALVING_INTERVAL) > 0,
                "reward at halving {} should be > 0", h);
        }
    }

    #[test]
    fn candidate_height_increments_parent() {
        let tip = genesis_tip();
        let now = tip.header.timestamp + TARGET_BLOCK_TIME;
        let job = build_candidate(&tip, 1, "miner1", vec![], &simple_window(&tip), now);
        assert_eq!(job.header_template.number, 1);
        assert_eq!(job.header_template.parent_hash, tip.hash());
    }

    #[test]
    fn candidate_has_coinbase_first() {
        let tip = genesis_tip();
        let now = tip.header.timestamp + TARGET_BLOCK_TIME;
        let job = build_candidate(&tip, 1, "miner1", vec![], &simple_window(&tip), now);
        let cb = &job.txs[0];
        assert_eq!(cb.module, "coinbase");
        assert_eq!(cb.method, "reward");
        let encoded = u64::from_be_bytes(cb.args.as_slice().try_into().unwrap());
        assert_eq!(encoded, 1u64);
    }

    #[test]
    fn candidate_tx_root_matches_txs() {
        let tip = genesis_tip();
        let now = tip.header.timestamp + TARGET_BLOCK_TIME;
        let job = build_candidate(&tip, 1, "miner1", vec![], &simple_window(&tip), now);

        // Recompute tx_root from the stored txs and confirm it matches the header.
        let expected = {
            let mut h = blake3::Hasher::new();
            for tx in &job.txs {
                h.update(tx.tx_id().as_bytes());
            }
            hex::encode(h.finalize().as_bytes())
        };
        assert_eq!(job.header_template.tx_root, expected);
    }

    #[test]
    fn try_nonce_returns_some_at_difficulty_floor() {
        // Build a job at DIFFICULTY_FLOOR = 1 (every hash satisfies the target).
        let tip = genesis_tip();
        let now = tip.header.timestamp + TARGET_BLOCK_TIME;
        let mut job = build_candidate(&tip, 1, "miner1", vec![], &simple_window(&tip), now);
        // Force difficulty to DIFFICULTY_FLOOR so we get an instant hit.
        job.header_template.difficulty = DIFFICULTY_FLOOR;
        job.target_difficulty = DIFFICULTY_FLOOR;

        let blk = job.try_nonce(0).expect("nonce 0 must yield a block at difficulty=1");
        assert_eq!(blk.header.number, 1);
        assert_eq!(blk.header.nonce, 0);
        assert!(!blk.header.pow_hash.is_empty());
    }

    #[test]
    fn try_nonce_wrong_returns_none_at_max_difficulty() {
        // At u64::MAX difficulty the target is effectively zero — no hash passes.
        let tip = genesis_tip();
        let now = tip.header.timestamp + TARGET_BLOCK_TIME;
        let mut job = build_candidate(&tip, 1, "miner1", vec![], &simple_window(&tip), now);
        job.header_template.difficulty = u64::MAX;
        job.target_difficulty = u64::MAX;

        assert!(job.try_nonce(0).is_none());
    }

    #[test]
    fn extra_txs_capped_at_block_target() {
        let tip = genesis_tip();
        let now = tip.header.timestamp + TARGET_BLOCK_TIME;
        // Supply more txs than the limit.
        let extra: Vec<Tx> = (0..BLOCK_TARGET_TXS + 50)
            .map(|i| Tx {
                nonce: i as u64,
                sender_pubkey: String::new(),
                module: "transfer".to_string(),
                method: "send".to_string(),
                args: vec![],
                tip: 0,
                fee_limit: 0,
                sig: String::new(),
            })
            .collect();
        let job = build_candidate(&tip, 1, "miner1", extra, &simple_window(&tip), now);
        assert!(job.txs.len() <= BLOCK_TARGET_TXS, "txs.len()={} should be ≤ BLOCK_TARGET_TXS={}", job.txs.len(), BLOCK_TARGET_TXS);
    }

    #[test]
    fn build_from_non_genesis_parent() {
        let gen = genesis_tip();
        // Use make_test_block to create a valid h=1 parent.
        let b1 = make_test_block(gen.hash(), 1, gen.header.timestamp + TARGET_BLOCK_TIME, 0xAA);
        let now = b1.header.timestamp + TARGET_BLOCK_TIME;
        let window = vec![gen.clone(), b1.clone()];
        let job = build_candidate(&b1, 2, "miner2", vec![], &window, now);
        assert_eq!(job.header_template.number, 2);
        assert_eq!(job.header_template.parent_hash, b1.hash());
    }
}

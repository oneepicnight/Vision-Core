use crate::chain::accept::{apply_block, AcceptResult};
use crate::chain::ChainState;
use crate::miner::job::{build_candidate_with_params, MiningJob};
use crate::pow::visionx::VisionXParams;
use crate::types::Tx;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Global monotonic job counter.
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

struct MinerInner {
    /// The current mining job broadcast to all workers. None if mining is paused.
    current_job: Option<MiningJob>,

    /// VisionX epoch for which the dataset was last built.
    /// Persists across `clear_job()` so the dataset is only rebuilt
    /// when the epoch changes, not on every chain tip update.
    last_built_epoch: Option<u64>,

    /// Running stats.
    blocks_found: u64,
    start_time: Instant,
}

/// Simple runtime statistics for the miner.
#[derive(Debug, Clone)]
pub struct MiningStats {
    /// Number of locally produced blocks that were accepted by `apply_block`.
    pub blocks_found: u64,
    /// Monotonic time when the miner was first started.
    pub start_time: Instant,
}

/// Coordinates mining worker threads and job distribution.
///
/// Workers poll `current_job()` and abandon work when the job_id changes.
pub struct MinerManager {
    inner: Arc<Mutex<MinerInner>>,
    params: VisionXParams,
}

impl MinerManager {
    pub fn new(params: VisionXParams) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MinerInner {
                current_job: None,
                last_built_epoch: None,
                blocks_found: 0,
                start_time: Instant::now(),
            })),
            params,
        }
    }

    /// Build and publish a new mining job.
    ///
    /// Only rebuilds the VisionX dataset (expensive) when the epoch changes.
    /// A tip update that stays in the same epoch reuses the cached dataset.
    pub fn build_job(&self, job: MiningJob) {
        let epoch = job.epoch;
        let mut inner = self.inner.lock().unwrap();
        let needs_dataset_rebuild = inner.last_built_epoch != Some(epoch);
        if needs_dataset_rebuild {
            tracing::info!("[MINER] Epoch {} → dataset rebuild", epoch);
            inner.last_built_epoch = Some(epoch);
        }
        tracing::debug!(
            "[MINER] New job id={} h={} diff={}",
            job.job_id,
            job.header_template.number,
            job.target_difficulty
        );
        inner.current_job = Some(job);
    }

    /// Clear the current job, pausing all workers.
    ///
    /// Does NOT clear `last_built_epoch` — if the next job is in the same
    /// epoch, the dataset is not rebuilt.
    pub fn clear_job(&self) {
        self.inner.lock().unwrap().current_job = None;
    }

    /// Return a clone of the current job, or None if mining is paused.
    pub fn current_job(&self) -> Option<MiningJob> {
        self.inner.lock().unwrap().current_job.clone()
    }

    /// Return the next available job id (monotonic).
    pub fn next_job_id() -> u64 {
        NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// True if a job is currently active.
    pub fn is_mining(&self) -> bool {
        self.inner.lock().unwrap().current_job.is_some()
    }

    /// Return a snapshot of the current mining statistics.
    pub fn stats(&self) -> MiningStats {
        let inner = self.inner.lock().unwrap();
        MiningStats {
            blocks_found: inner.blocks_found,
            start_time: inner.start_time,
        }
    }

    /// Record one successfully accepted block.  Called internally by
    /// `submit_solution` on `CanonExtension`.
    fn record_block_found(&self) {
        self.inner.lock().unwrap().blocks_found += 1;
    }

    /// Submit a locally-found block through the standard acceptance pipeline.
    ///
    /// This is the **only** path for integrating a mined block.  No special
    /// fast-path exists; the block goes through every stage of `apply_block`
    /// identically to a block received from a peer.
    ///
    /// Returns the `AcceptResult` from `apply_block`.
    pub fn submit_solution(&self, g: &mut ChainState, block: crate::types::Block) -> AcceptResult {
        let result = apply_block(g, &block, None);
        if matches!(result, AcceptResult::CanonExtension { .. }) {
            self.record_block_found();
        }
        result
    }

    /// Build a candidate `MiningJob` for the current canonical tip.
    ///
    /// Returns `None` if the chain is empty (no canonical tip yet).
    pub fn build_candidate_for_tip(
        &self,
        g: &ChainState,
        miner_addr: &str,
        mempool_txs: Vec<Tx>,
    ) -> Option<MiningJob> {
        let tip = g.blocks.last()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .max(tip.header.timestamp + 1); // always strictly greater than parent

        // Collect ancestor window for difficulty calculation.
        let limit = (crate::config::constants::RETARGET_WINDOW + 1) as usize;
        let window: Vec<crate::types::Block> = g
            .blocks
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        let job_id = Self::next_job_id();
        Some(build_candidate_with_params(
            tip,
            job_id,
            miner_addr,
            mempool_txs,
            &window,
            now,
            self.params,
        ))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::tests_helpers::make_test_block;
    use crate::chain::accept::AcceptResult;
    use crate::config::constants::{DIFFICULTY_FLOOR, TARGET_BLOCK_TIME};
    use crate::genesis::genesis_block;
    use crate::pow::visionx::VISIONX_PARAMS;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn seeded_state() -> ChainState {
        let mut g = temp_state();
        let gen = genesis_block();
        apply_block(&mut g, &gen, None);
        g
    }

    fn default_manager() -> MinerManager {
        MinerManager::new(*VISIONX_PARAMS)
    }

    // ── Job distribution ──────────────────────────────────────────────────────

    #[test]
    fn no_job_initially() {
        let m = default_manager();
        assert!(!m.is_mining());
        assert!(m.current_job().is_none());
    }

    #[test]
    fn build_and_clear_job() {
        let m = default_manager();
        let g = seeded_state();
        let job = m
            .build_candidate_for_tip(&g, "addr1", vec![])
            .expect("should produce a job when chain has genesis");
        m.build_job(job);
        assert!(m.is_mining());
        m.clear_job();
        assert!(!m.is_mining());
    }

    #[test]
    fn job_id_is_monotonically_increasing() {
        let a = MinerManager::next_job_id();
        let b = MinerManager::next_job_id();
        let c = MinerManager::next_job_id();
        assert!(a < b && b < c);
    }

    #[test]
    fn build_candidate_returns_none_when_no_tip() {
        let m = default_manager();
        let g = temp_state(); // empty — no genesis
        assert!(m.build_candidate_for_tip(&g, "addr", vec![]).is_none());
    }

    #[test]
    fn build_candidate_has_correct_parent_and_height() {
        let m = default_manager();
        let g = seeded_state();
        let job = m.build_candidate_for_tip(&g, "miner", vec![]).unwrap();
        let gen = genesis_block();
        assert_eq!(job.header_template.number, 1);
        assert_eq!(job.header_template.parent_hash, gen.hash());
    }

    // ── Stats ─────────────────────────────────────────────────────────────────

    #[test]
    fn stats_starts_at_zero_blocks_found() {
        let m = default_manager();
        assert_eq!(m.stats().blocks_found, 0);
    }

    // ── Submit solution — locally mined block through apply_block ─────────────

    /// A locally-found block must pass through apply_block the same as a peer block.
    /// We use make_test_block (DIFFICULTY_FLOOR = 1, known-good hash) to avoid
    /// running real VisionX hashing in tests.
    #[test]
    fn locally_mined_block_accepted_via_apply_block() {
        let m = default_manager();
        let mut g = seeded_state();
        let gen = genesis_block();
        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        // make_test_block produces a block at DIFFICULTY_FLOOR with valid PoW.
        let blk = make_test_block(gen.hash(), 1, ts, 0xAA);
        let result = m.submit_solution(&mut g, blk);
        assert_eq!(result, AcceptResult::CanonExtension { height: 1 });
    }

    #[test]
    fn submit_solution_increments_blocks_found() {
        let m = default_manager();
        let mut g = seeded_state();
        let gen = genesis_block();
        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let blk = make_test_block(gen.hash(), 1, ts, 0xAA);
        m.submit_solution(&mut g, blk);
        assert_eq!(m.stats().blocks_found, 1);
    }

    #[test]
    fn submit_solution_returns_canon_extension() {
        let m = default_manager();
        let mut g = seeded_state();
        let gen = genesis_block();
        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let blk = make_test_block(gen.hash(), 1, ts, 0xBB);
        match m.submit_solution(&mut g, blk) {
            AcceptResult::CanonExtension { height: 1 } => {}
            other => panic!("expected CanonExtension{{height:1}}, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_block_rejected_by_accept() {
        let m = default_manager();
        let mut g = seeded_state();
        let gen = genesis_block();
        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        let blk = make_test_block(gen.hash(), 1, ts, 0xCC);
        let r1 = m.submit_solution(&mut g, blk.clone());
        let r2 = m.submit_solution(&mut g, blk);
        assert_eq!(r1, AcceptResult::CanonExtension { height: 1 });
        assert!(matches!(r2, AcceptResult::Rejected(_)));
    }

    #[test]
    fn multiple_blocks_extend_chain() {
        let m = default_manager();
        let mut g = seeded_state();
        let gen = genesis_block();
        let mut prev_hash = gen.hash().to_string();
        let mut ts = gen.header.timestamp;
        for i in 1u64..=4 {
            ts += TARGET_BLOCK_TIME;
            let blk = make_test_block(&prev_hash, i, ts, (0xA0 + i) as u8);
            let r = m.submit_solution(&mut g, blk.clone());
            assert_eq!(r, AcceptResult::CanonExtension { height: i });
            prev_hash = blk.hash().to_string();
        }
        assert_eq!(m.stats().blocks_found, 4);
    }

    #[test]
    fn invalid_block_rejected_and_stats_unchanged() {
        let m = default_manager();
        let mut g = seeded_state();
        let gen = genesis_block();
        let ts = gen.header.timestamp + TARGET_BLOCK_TIME;
        // Use an unknown parent hash so the block is stored as an orphan.
        // OrphanStored ≠ CanonExtension, so blocks_found must not increment.
        let bad = make_test_block(&"cc".repeat(32), 1, ts, 0xAA);
        let r = m.submit_solution(&mut g, bad);
        assert!(
            matches!(r, AcceptResult::StoredOrphan { .. }),
            "expected StoredOrphan, got {:?}",
            r
        );
        assert_eq!(m.stats().blocks_found, 0);
    }
}

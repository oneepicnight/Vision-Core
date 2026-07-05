use std::time::Instant;
use anyhow::Result;
use crate::p2p::peer_manager::PeerManager;
use crate::config::constants::{SYNC_LAG_THRESHOLD, SYNC_CLEAR_JOB_MIN_LAG, STALL_OVERRIDE_SECS};

// ─── SyncDecision ─────────────────────────────────────────────────────────────

/// Result returned by `should_sync`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDecision {
    /// Local tip is at or ahead of all known peers; no action required.
    Synced,

    /// Local tip is behind `peer_addr` by `lag` blocks.
    ///
    /// The caller should initiate a catch-up from `peer_addr`.
    Behind {
        /// Address of the best peer to sync from.
        peer_addr: String,
        /// Number of blocks the local chain lags behind.
        lag: u64,
    },
}

/// Decide whether the local chain needs to sync and, if so, who to sync from.
///
/// Returns `Behind` only when:
/// - There is a connected, fresh peer whose height exceeds `local_height`
/// - The gap is at least `SYNC_LAG_THRESHOLD` blocks
///
/// Gaps smaller than the threshold are tolerated without triggering sync
/// (prevents oscillation when the node is near the tip).
pub fn should_sync(peer_manager: &PeerManager, local_height: u64) -> SyncDecision {
    let remote_height = peer_manager.best_remote_height();
    let lag = remote_height.saturating_sub(local_height);

    if lag < SYNC_LAG_THRESHOLD {
        return SyncDecision::Synced;
    }

    match peer_manager.best_sync_target(local_height) {
        Some(peer_addr) => SyncDecision::Behind { peer_addr, lag },
        // No qualified target despite the height gap (e.g. all matching peers
        // just disconnected). Treat as Synced to avoid a crash-loop.
        None => SyncDecision::Synced,
    }
}

// ─── SyncGuard ────────────────────────────────────────────────────────────────

/// Prevents sync thrashing by tracking whether a sync is already in progress
/// and enforcing a cooldown between consecutive sync attempts.
///
/// The guard is intentionally not `Clone` (one guard per node).
pub struct SyncGuard {
    in_progress: bool,
    /// Earliest time we may start a new sync after the previous one finished.
    cooldown_until: Option<Instant>,
}

impl SyncGuard {
    pub fn new() -> Self {
        Self { in_progress: false, cooldown_until: None }
    }

    /// `true` while a sync is running — i.e. between `mark_started` and
    /// `mark_done`.
    pub fn is_in_progress(&self) -> bool {
        self.in_progress
    }

    /// `true` if we are within the post-sync cooldown window.
    ///
    /// Callers should check this **and** `is_in_progress` before re-triggering.
    pub fn is_throttled(&self) -> bool {
        self.cooldown_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    /// `true` if a new sync attempt should be blocked right now.
    ///
    /// Equivalent to `is_in_progress() || is_throttled()`.
    pub fn is_blocked(&self) -> bool {
        self.is_in_progress() || self.is_throttled()
    }

    /// Signal that a sync has started.
    pub fn mark_started(&mut self) {
        self.in_progress = true;
    }

    /// Signal that a sync has finished (successfully or not).
    ///
    /// Sets a cooldown of `STALL_OVERRIDE_SECS` seconds before the next
    /// sync may start.
    pub fn mark_done(&mut self) {
        self.in_progress = false;
        self.cooldown_until = Some(
            Instant::now() + std::time::Duration::from_secs(STALL_OVERRIDE_SECS),
        );
    }

    /// Force-clear both the in-progress flag and the cooldown.
    ///
    /// Use only in tests or when an external signal explicitly re-enables sync.
    #[cfg(test)]
    pub fn reset(&mut self) {
        self.in_progress = false;
        self.cooldown_until = None;
    }
}

impl Default for SyncGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Watchdog ─────────────────────────────────────────────────────────────────

/// Sync watchdog step — call this on a periodic timer.
///
/// Evaluates `should_sync`, fires a catch-up if needed, and respects the
/// `SyncGuard` to avoid concurrent or thrashing syncs.
///
/// `perform_catchup` is an async closure so the sync transport layer can
/// be injected (no hard dependency on TCP here).
pub async fn watchdog_step(
    peer_manager: &PeerManager,
    local_height: u64,
    guard: &mut SyncGuard,
) -> Result<()> {
    if guard.is_blocked() {
        tracing::trace!("[SYNC] watchdog skipped (sync in progress or throttled)");
        return Ok(());
    }

    match should_sync(peer_manager, local_height) {
        SyncDecision::Synced => {
            tracing::trace!("[SYNC] up to date (local h={})", local_height);
        }
        SyncDecision::Behind { peer_addr, lag } => {
            tracing::info!(
                "[SYNC] starting catchup from {} lag={} local h={}",
                peer_addr, lag, local_height
            );

            if lag >= SYNC_CLEAR_JOB_MIN_LAG {
                tracing::info!("[SYNC] clearing miner job (lag={})", lag);
                // Miner handle will be wired in the node service layer.
            }

            // Mark in-progress so the next watchdog tick cannot double-trigger.
            guard.mark_started();

            // TODO Prompt 10+: call actual TCP catchup here.
            // For now signal done immediately; real impl would await transport.
            guard.mark_done();
        }
    }

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::peer_manager::{PeerManager, PeerState};

    /// Build a peer manager with some connected, fresh peers.
    fn pm_with(peers: &[(&str, u64)]) -> PeerManager {
        let pm = PeerManager::new();
        for &(addr, height) in peers {
            pm.upsert(addr, true);
            pm.set_state(addr, PeerState::Connected);
            pm.note_peer_height(addr, height, false);
        }
        pm
    }

    // ── should_sync ───────────────────────────────────────────────────────────

    #[test]
    fn synced_when_no_peers() {
        let pm = PeerManager::new();
        assert_eq!(should_sync(&pm, 0), SyncDecision::Synced);
    }

    #[test]
    fn synced_when_local_matches_best_peer() {
        let pm = pm_with(&[("a:9000", 100)]);
        assert_eq!(should_sync(&pm, 100), SyncDecision::Synced);
    }

    #[test]
    fn synced_when_local_is_ahead() {
        let pm = pm_with(&[("a:9000", 50)]);
        assert_eq!(should_sync(&pm, 100), SyncDecision::Synced);
    }

    #[test]
    fn synced_when_lag_is_below_threshold() {
        // SYNC_LAG_THRESHOLD = 5; lag of 4 should not trigger sync.
        let pm = pm_with(&[("a:9000", 104)]);
        assert_eq!(should_sync(&pm, 100), SyncDecision::Synced);
    }

    #[test]
    fn behind_when_lag_equals_threshold() {
        // Lag exactly at threshold triggers sync.
        let local_h = 100u64;
        let peer_h  = local_h + SYNC_LAG_THRESHOLD;
        let pm = pm_with(&[("a:9000", peer_h)]);
        match should_sync(&pm, local_h) {
            SyncDecision::Behind { lag, .. } => assert_eq!(lag, SYNC_LAG_THRESHOLD),
            other => panic!("expected Behind, got {:?}", other),
        }
    }

    #[test]
    fn behind_returns_correct_peer_and_lag() {
        let pm = pm_with(&[("a:9000", 50), ("b:9000", 200), ("c:9000", 80)]);
        match should_sync(&pm, 0) {
            SyncDecision::Behind { peer_addr, lag } => {
                assert_eq!(peer_addr, "b:9000");
                assert_eq!(lag, 200);
            }
            other => panic!("expected Behind, got {:?}", other),
        }
    }

    #[test]
    fn best_peer_tiebreak_is_deterministic() {
        // Two peers at equal height — smallest addr wins.
        let pm = pm_with(&[("zzz:9000", 100), ("aaa:9000", 100)]);
        match should_sync(&pm, 0) {
            SyncDecision::Behind { peer_addr, .. } => assert_eq!(peer_addr, "aaa:9000"),
            other => panic!("expected Behind, got {:?}", other),
        }
    }

    #[test]
    fn stale_peers_are_not_picked_as_sync_target() {
        // We cannot fast-forward Instant, but we can verify that peers with
        // height=0 (never polled) are excluded.
        let pm = PeerManager::new();
        pm.upsert("a:9000", true);
        pm.set_state("a:9000", PeerState::Connected);
        // note_peer_height not called → height=0, last_height_updated_at=None
        // → is_height_fresh() returns false
        assert_eq!(should_sync(&pm, 0), SyncDecision::Synced);
    }

    // ── SyncGuard ─────────────────────────────────────────────────────────────

    #[test]
    fn guard_initially_not_blocked() {
        let g = SyncGuard::new();
        assert!(!g.is_in_progress());
        assert!(!g.is_throttled());
        assert!(!g.is_blocked());
    }

    #[test]
    fn guard_blocks_while_in_progress() {
        let mut g = SyncGuard::new();
        g.mark_started();
        assert!(g.is_in_progress());
        assert!(g.is_blocked());
    }

    #[test]
    fn guard_unblocked_after_reset() {
        let mut g = SyncGuard::new();
        g.mark_started();
        g.reset();
        assert!(!g.is_blocked());
    }

    #[test]
    fn guard_mark_done_clears_in_progress() {
        let mut g = SyncGuard::new();
        g.mark_started();
        assert!(g.is_in_progress());
        g.mark_done();
        assert!(!g.is_in_progress());
        // Note: is_throttled() will be true right after mark_done due to
        // cooldown. That is correct behaviour — we just confirm in_progress
        // is cleared without waiting out the cooldown.
    }

    #[test]
    fn guard_is_throttled_immediately_after_done() {
        let mut g = SyncGuard::new();
        g.mark_started();
        g.mark_done();
        // Cooldown fires immediately after done — still blocked.
        assert!(g.is_blocked());
    }

    // ── watchdog_step ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn watchdog_noop_when_synced() {
        let pm = pm_with(&[("a:9000", 5)]);
        let mut guard = SyncGuard::new();
        watchdog_step(&pm, 5, &mut guard).await.unwrap();
        // Guard is neither in_progress nor throttled (no sync was triggered).
        assert!(!guard.is_in_progress());
    }

    #[tokio::test]
    async fn watchdog_sets_then_clears_in_progress_when_behind() {
        let local_h = 0u64;
        let peer_h  = local_h + SYNC_LAG_THRESHOLD + 1;
        let pm = pm_with(&[("a:9000", peer_h)]);
        let mut guard = SyncGuard::new();
        watchdog_step(&pm, local_h, &mut guard).await.unwrap();
        // After the stub sync completes, in_progress is cleared.
        assert!(!guard.is_in_progress());
        // But cooldown should be active.
        assert!(guard.is_throttled());
    }

    #[tokio::test]
    async fn watchdog_skips_when_guard_blocked() {
        let pm = pm_with(&[("a:9000", 1000)]);
        let mut guard = SyncGuard::new();
        guard.mark_started(); // Simulate concurrent sync.
        // Should not panic and should leave guard state unchanged.
        watchdog_step(&pm, 0, &mut guard).await.unwrap();
        assert!(guard.is_in_progress()); // Still in-progress — not cleared.
    }
}

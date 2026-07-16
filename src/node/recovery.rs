use std::sync::{Arc, RwLock};
use std::time::Instant;

use serde::Serialize;

use crate::p2p::protocol::ChainSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMode {
    Normal,
    HigherWorkRecovery,
    RecoveryLimited,
    HighRiskFork,
}

impl RecoveryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RecoveryMode::Normal => "normal",
            RecoveryMode::HigherWorkRecovery => "higher_work_recovery",
            RecoveryMode::RecoveryLimited => "recovery_limited",
            RecoveryMode::HighRiskFork => "high_risk_fork",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveryStatusSnapshot {
    pub state: &'static str,
    pub peer_addr: Option<String>,
    pub local_height: Option<u64>,
    pub local_work: Option<u128>,
    pub local_tip_hash: Option<String>,
    pub remote_height: Option<u64>,
    pub remote_work: Option<u128>,
    pub remote_tip_hash: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
struct RecoveryClaim {
    mode: RecoveryMode,
    peer_addr: String,
    local_summary: ChainSummary,
    remote_summary: ChainSummary,
    reason: String,
    updated_at: Instant,
}

#[derive(Debug, Default)]
struct RecoveryInner {
    claim: Option<RecoveryClaim>,
}

#[derive(Debug, Clone, Default)]
pub struct RecoveryState {
    inner: Arc<RwLock<RecoveryInner>>,
}

impl RecoveryState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_higher_work_recovery(
        &self,
        peer_addr: &str,
        local_summary: ChainSummary,
        remote_summary: ChainSummary,
    ) {
        let mut inner = self.inner.write().unwrap();
        tracing::warn!(
            "[RECOVERY] higher-work recovery started peer={} local_h={} local_work={} remote_h={} remote_work={} local_tip={:?} remote_tip={:?}",
            peer_addr,
            local_summary.height,
            local_summary.cumulative_work,
            remote_summary.height,
            remote_summary.cumulative_work,
            local_summary.tip_hash,
            remote_summary.tip_hash
        );
        inner.claim = Some(RecoveryClaim {
            mode: RecoveryMode::HigherWorkRecovery,
            peer_addr: peer_addr.to_string(),
            local_summary,
            remote_summary,
            reason: "peer advertised strictly greater cumulative work".to_string(),
            updated_at: Instant::now(),
        });
    }

    pub fn mark_limited(&self, reason: impl Into<String>) {
        let mut inner = self.inner.write().unwrap();
        if let Some(claim) = inner.claim.as_mut() {
            claim.mode = RecoveryMode::RecoveryLimited;
            claim.reason = reason.into();
            claim.updated_at = Instant::now();
            tracing::warn!("[RECOVERY] recovery limited: {}", claim.reason);
        }
    }

    pub fn mark_high_risk(&self, reason: impl Into<String>) {
        let mut inner = self.inner.write().unwrap();
        if let Some(claim) = inner.claim.as_mut() {
            claim.mode = RecoveryMode::HighRiskFork;
            claim.reason = reason.into();
            claim.updated_at = Instant::now();
            tracing::error!("[RECOVERY] high-risk fork: {}", claim.reason);
        }
    }

    pub fn clear(&self, reason: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.claim.is_some() {
            tracing::info!("[RECOVERY] recovery cleared: {}", reason);
        }
        inner.claim = None;
    }

    pub fn should_pause_mining(&self) -> bool {
        self.inner.read().unwrap().claim.is_some()
    }

    pub fn mode(&self) -> RecoveryMode {
        self.inner
            .read()
            .unwrap()
            .claim
            .as_ref()
            .map(|claim| claim.mode)
            .unwrap_or(RecoveryMode::Normal)
    }

    pub fn snapshot(&self) -> RecoveryStatusSnapshot {
        let inner = self.inner.read().unwrap();
        match inner.claim.as_ref() {
            Some(claim) => RecoveryStatusSnapshot {
                state: claim.mode.as_str(),
                peer_addr: Some(claim.peer_addr.clone()),
                local_height: Some(claim.local_summary.height),
                local_work: Some(claim.local_summary.cumulative_work),
                local_tip_hash: claim.local_summary.tip_hash.clone(),
                remote_height: Some(claim.remote_summary.height),
                remote_work: Some(claim.remote_summary.cumulative_work),
                remote_tip_hash: claim.remote_summary.tip_hash.clone(),
                reason: Some(claim.reason.clone()),
            },
            None => RecoveryStatusSnapshot {
                state: RecoveryMode::Normal.as_str(),
                peer_addr: None,
                local_height: None,
                local_work: None,
                local_tip_hash: None,
                remote_height: None,
                remote_work: None,
                remote_tip_hash: None,
                reason: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(height: u64, tip: &str, work: u128) -> ChainSummary {
        ChainSummary::new(height, Some(tip.to_string()), work)
    }

    #[test]
    fn normal_state_does_not_pause_mining() {
        let recovery = RecoveryState::new();
        assert!(!recovery.should_pause_mining());
        assert_eq!(recovery.mode(), RecoveryMode::Normal);
        assert_eq!(recovery.snapshot().state, "normal");
    }

    #[test]
    fn higher_work_recovery_pauses_mining() {
        let recovery = RecoveryState::new();
        recovery.begin_higher_work_recovery(
            "127.0.0.1:9001",
            summary(83, "local", 1754),
            summary(81, "remote", 1757),
        );
        assert!(recovery.should_pause_mining());
        assert_eq!(recovery.mode(), RecoveryMode::HigherWorkRecovery);
        let snapshot = recovery.snapshot();
        assert_eq!(snapshot.state, "higher_work_recovery");
        assert_eq!(snapshot.peer_addr.as_deref(), Some("127.0.0.1:9001"));
        assert_eq!(snapshot.remote_work, Some(1757));
    }

    #[test]
    fn limited_and_high_risk_states_continue_pausing_mining() {
        let recovery = RecoveryState::new();
        recovery.begin_higher_work_recovery("peer", summary(1, "a", 1), summary(2, "b", 2));
        recovery.mark_limited("batch budget exhausted");
        assert!(recovery.should_pause_mining());
        assert_eq!(recovery.mode(), RecoveryMode::RecoveryLimited);
        recovery.mark_high_risk("branch unavailable");
        assert!(recovery.should_pause_mining());
        assert_eq!(recovery.mode(), RecoveryMode::HighRiskFork);
    }

    #[test]
    fn clear_resumes_mining() {
        let recovery = RecoveryState::new();
        recovery.begin_higher_work_recovery("peer", summary(1, "a", 1), summary(2, "b", 2));
        recovery.clear("adopted");
        assert!(!recovery.should_pause_mining());
        assert_eq!(recovery.mode(), RecoveryMode::Normal);
    }
}

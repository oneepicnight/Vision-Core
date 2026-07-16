use axum::{extract::State, Json};
use serde::Serialize;

use crate::api::state::NodeApiState;

/// Mining status response.
#[derive(Serialize)]
pub struct MiningInfoResponse {
    pub enabled: bool,
    pub height: u64,
    pub difficulty: u64,
    pub epoch: u64,
    pub active: bool,
    pub recovery_state: &'static str,
    pub paused_reason: Option<String>,
    pub hash_rate_estimate: Option<f64>,
}

/// GET /mining/info
pub async fn get_mining_info(State(state): State<NodeApiState>) -> Json<MiningInfoResponse> {
    // Read-only view of the live runtime miner state.
    Json(state.mining_info_snapshot().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::state::NodeApiState;
    use crate::chain::ChainState;
    use crate::genesis::genesis_block;
    use crate::mempool::Mempool;
    use crate::miner::MinerManager;
    use crate::pow::visionx::VisionXParams;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    #[tokio::test]
    async fn mining_info_snapshot_reflects_runtime_state() {
        let chain = Arc::new(Mutex::new(temp_state()));
        {
            let mut chain_guard = chain.lock().await;
            chain_guard.blocks.push(genesis_block());
            chain_guard.difficulty = 7;
        }
        let state = NodeApiState::new(chain, Arc::new(Mempool::new()))
            .with_miner_manager(Arc::new(MinerManager::new(VisionXParams::default())));

        let snapshot = state.mining_info_snapshot().await;

        assert!(snapshot.enabled);
        assert_eq!(snapshot.height, 0);
        assert_eq!(snapshot.difficulty, 7);
        assert_eq!(snapshot.epoch, 0);
        assert!(!snapshot.active);
        assert_eq!(snapshot.recovery_state, "normal");
        assert_eq!(snapshot.paused_reason, None);
        assert_eq!(snapshot.hash_rate_estimate, None);
    }

    #[tokio::test]
    async fn mining_info_reports_higher_work_recovery_pause() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let recovery = Arc::new(crate::node::recovery::RecoveryState::new());
        recovery.begin_higher_work_recovery(
            "127.0.0.1:9009",
            crate::p2p::protocol::ChainSummary::new(1, Some("local".to_string()), 1),
            crate::p2p::protocol::ChainSummary::new(2, Some("remote".to_string()), 2),
        );
        let state = NodeApiState::new(chain, Arc::new(Mempool::new()))
            .with_miner_manager(Arc::new(MinerManager::new(VisionXParams::default())))
            .with_recovery_state(recovery);

        let snapshot = state.mining_info_snapshot().await;

        assert!(snapshot.enabled);
        assert!(!snapshot.active);
        assert_eq!(snapshot.recovery_state, "higher_work_recovery");
        assert!(snapshot.paused_reason.is_some());
    }
    #[tokio::test]
    async fn mining_info_endpoint_matches_snapshot() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let state = NodeApiState::new(chain, Arc::new(Mempool::new()));

        let Json(response) = get_mining_info(State(state.clone())).await;
        let expected = state.mining_info_snapshot().await;

        assert_eq!(response.enabled, expected.enabled);
        assert_eq!(response.height, expected.height);
        assert_eq!(response.difficulty, expected.difficulty);
        assert_eq!(response.epoch, expected.epoch);
        assert_eq!(response.active, expected.active);
        assert_eq!(response.recovery_state, expected.recovery_state);
        assert_eq!(response.paused_reason, expected.paused_reason);
        assert_eq!(response.hash_rate_estimate, expected.hash_rate_estimate);
    }
}

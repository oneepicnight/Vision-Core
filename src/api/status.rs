use axum::{extract::State, Json};

use crate::api::state::{NodeApiState, NodeStatusSnapshot};

pub(crate) type StatusResponse = NodeStatusSnapshot;

/// GET /status
///
/// Returns a read-only snapshot of the node's current state.
pub(crate) async fn get_status(State(state): State<NodeApiState>) -> Json<StatusResponse> {
    Json(state.status_snapshot().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::ChainState;
    use crate::mempool::Mempool;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    #[tokio::test]
    async fn status_handler_returns_state_snapshot() {
        let state = NodeApiState::new(Arc::new(Mutex::new(temp_state())), Arc::new(Mempool::new()));

        let Json(response) = get_status(State(state.clone())).await;

        assert_eq!(response, state.status_snapshot().await);
    }
}

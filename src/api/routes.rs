use axum::{routing::get, Router};

use crate::api::state::NodeApiState;
use crate::api::{mining, peers, status};

/// Build the HTTP API router with all registered routes.
pub(crate) fn api_router(state: NodeApiState) -> Router {
    Router::new()
        .route("/status", get(status::get_status))
        .route("/peers", get(peers::list_peers))
        .route("/mining/info", get(mining::get_mining_info))
        .with_state(state)
}

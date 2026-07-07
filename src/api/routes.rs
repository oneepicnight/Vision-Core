use axum::{routing::{get, post}, Router};

use crate::api::state::NodeApiState;
use crate::api::{mining, peers, status, transactions};

/// Build the HTTP API router with all registered routes.
pub(crate) fn api_router(state: NodeApiState) -> Router {
    Router::new()
        .route("/status", get(status::get_status))
        .route("/peers", get(peers::list_peers))
        .route("/mining/info", get(mining::get_mining_info))
        .route("/transactions", post(transactions::submit_transaction_http))
        .with_state(state)
}

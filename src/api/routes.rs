use axum::{routing::{get, post}, Router};

use crate::api::state::NodeApiState;
use crate::api::{alpha, mining, peers, read_only, status, transactions};

/// Build the HTTP API router with all registered routes.
pub(crate) fn api_router(state: NodeApiState) -> Router {
    let mut router = Router::new()
        .route("/status", get(status::get_status))
        .route("/peers", get(peers::list_peers))
        .route("/balance/:address", get(read_only::get_balance))
        .route("/nonce/:address", get(read_only::get_nonce))
        .route("/transaction/:txid", get(read_only::get_transaction))
        .route("/mining/info", get(mining::get_mining_info))
        .route("/transactions", post(transactions::submit_transaction_http));

    if state.alpha_airdrop_enabled() {
        router = router.route("/alpha/airdrop", post(alpha::post_alpha_airdrop));
    }

    router.with_state(state)
}

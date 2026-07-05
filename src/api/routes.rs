use axum::{routing::get, Router};
use crate::api::{status, mining, peers};

/// Build the HTTP API router with all registered routes.
pub fn api_router() -> Router {
    Router::new()
        .route("/status",       get(status::get_status))
        .route("/peers",        get(peers::list_peers))
        .route("/mining/info",  get(mining::get_mining_info))
}

use axum::Json;
use serde::Serialize;

/// Node status response.
#[derive(Serialize)]
pub struct StatusResponse {
    pub version: &'static str,
    pub height: u64,
    pub tip_hash: String,
    pub difficulty: u64,
    pub peer_count: usize,
    pub mining: bool,
}

/// GET /status
///
/// Returns a summary of the node's current state. Safe to call at any time.
pub async fn get_status() -> Json<StatusResponse> {
    // Stub: real implementation wires into shared ChainState via Axum State extractor.
    Json(StatusResponse {
        version:    crate::config::constants::NODE_VERSION,
        height:     0,
        tip_hash:   crate::genesis::genesis::GENESIS_HASH.to_string(),
        difficulty: 1,
        peer_count: 0,
        mining:     false,
    })
}

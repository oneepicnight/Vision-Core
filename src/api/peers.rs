use axum::Json;
use serde::Serialize;

/// Single-peer entry in the peers list response.
#[derive(Serialize)]
pub struct PeerEntry {
    pub addr: String,
    pub state: String,
    pub height: u64,
    pub outbound: bool,
    pub height_age_secs: Option<u64>,
}

/// GET /peers
pub async fn list_peers() -> Json<Vec<PeerEntry>> {
    // Stub: wire into PeerManager via Axum State extractor.
    Json(vec![])
}

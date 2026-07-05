use axum::Json;
use serde::Serialize;

/// Mining status response.
#[derive(Serialize)]
pub struct MiningInfoResponse {
    pub enabled: bool,
    pub height: u64,
    pub difficulty: u64,
    pub epoch: u64,
    pub hash_rate_estimate: Option<f64>,
}

/// GET /mining/info
pub async fn get_mining_info() -> Json<MiningInfoResponse> {
    // Stub: wire into MinerManager via Axum State extractor.
    Json(MiningInfoResponse {
        enabled:            false,
        height:             0,
        difficulty:         1,
        epoch:              0,
        hash_rate_estimate: None,
    })
}

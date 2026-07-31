use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::state::{AlphaAirdropError, AlphaAirdropSnapshot, NodeApiState};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct AlphaAirdropRequest {
    pub address: String,
    pub amount: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AlphaAirdropHttpError {
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AlphaAirdropHttpResponse {
    pub status: &'static str,
    pub scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_tip_height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AlphaAirdropHttpError>,
}

impl AlphaAirdropHttpResponse {
    fn accepted(snapshot: AlphaAirdropSnapshot) -> Self {
        Self {
            status: snapshot.status,
            scope: snapshot.scope,
            address: Some(snapshot.address),
            amount: Some(snapshot.amount),
            balance: Some(snapshot.balance),
            canonical_tip_height: Some(snapshot.canonical_tip_height),
            cached_state_root: Some(snapshot.cached_state_root),
            error: None,
        }
    }

    fn rejected(code: &'static str, message: &'static str) -> Self {
        Self {
            status: "rejected",
            scope: "alpha_dev_only",
            address: None,
            amount: None,
            balance: None,
            canonical_tip_height: None,
            cached_state_root: None,
            error: Some(AlphaAirdropHttpError { code, message }),
        }
    }

    fn malformed_request() -> Self {
        Self {
            status: "malformed_request",
            scope: "alpha_dev_only",
            address: None,
            amount: None,
            balance: None,
            canonical_tip_height: None,
            cached_state_root: None,
            error: Some(AlphaAirdropHttpError {
                code: "malformed_request",
                message: "request body must be a JSON object with address and amount",
            }),
        }
    }
}

/// POST /alpha/airdrop
///
/// Alpha/dev only local funding endpoint. Disabled unless explicitly enabled
/// by `VISION_ALPHA_AIRDROP_ENABLED=true`.
pub(crate) async fn post_alpha_airdrop(State(state): State<NodeApiState>, body: Bytes) -> Response {
    let request: AlphaAirdropRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AlphaAirdropHttpResponse::malformed_request()),
            )
                .into_response();
        }
    };

    tracing::warn!(
        target: "vision.alpha",
        "[ALPHA/DEV ONLY] local airdrop request address={} amount={}",
        request.address,
        request.amount
    );

    match state.alpha_airdrop(&request.address, request.amount).await {
        Ok(snapshot) => (
            StatusCode::OK,
            Json(AlphaAirdropHttpResponse::accepted(snapshot)),
        )
            .into_response(),
        Err(AlphaAirdropError::Disabled) => (
            StatusCode::NOT_FOUND,
            Json(AlphaAirdropHttpResponse::rejected(
                "airdrop_disabled",
                "alpha local funding is disabled unless VISION_ALPHA_AIRDROP_ENABLED=true",
            )),
        )
            .into_response(),
        Err(AlphaAirdropError::InvalidAddress) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(AlphaAirdropHttpResponse::rejected(
                "invalid_address",
                "address must be a 64-character lowercase hex string",
            )),
        )
            .into_response(),
        Err(AlphaAirdropError::ZeroAmount) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(AlphaAirdropHttpResponse::rejected(
                "zero_amount",
                "amount must be greater than zero",
            )),
        )
            .into_response(),
        Err(AlphaAirdropError::StateRootComputationFailed) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AlphaAirdropHttpResponse::rejected(
                "state_root_error",
                "could not recompute canonical state root after funding",
            )),
        )
            .into_response(),
        Err(AlphaAirdropError::SnapshotPersistenceFailed) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AlphaAirdropHttpResponse::rejected(
                "snapshot_persistence_error",
                "could not persist alpha funding snapshot",
            )),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{self, Body},
        http::Request,
        Router,
    };
    use std::{path::Path, sync::Arc};
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    use crate::{
        api::routes::api_router,
        chain::{snapshots::restore_latest_snapshot, state::ChainState},
        config::settings::Settings,
        mempool::Mempool,
    };

    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn settings_for(dir: &Path, enabled: bool) -> Settings {
        Settings {
            data_dir: dir.display().to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            p2p_addr: "127.0.0.1:0".to_string(),
            p2p_auto_port: false,
            p2p_advertised_host: None,
            p2p_advertised_port: None,
            p2p_advertised_port_auto: false,
            allow_private_peer_addresses: true,
            miner_address: "0".repeat(64),
            mining_enabled: false,
            mining_threads: 0,
            alpha_airdrop_enabled: enabled,
            seed_peers: vec![],
        }
    }

    fn open_chain(dir: &TempDir) -> ChainState {
        let settings = settings_for(dir.path(), false);
        let mut chain = ChainState::open_with_genesis(&settings.data_dir).unwrap();
        crate::node::bootstrap::bootstrap_chain(&mut chain, &settings).unwrap();
        let genesis_hash = chain.block_at(0).unwrap().hash().to_string();
        crate::chain::storage::store_height_index(&chain, 0, &genesis_hash).unwrap();
        let current_height = chain.current_height();
        let _ = restore_latest_snapshot(&mut chain, current_height);
        chain
    }

    fn router_for(chain: ChainState, enabled: bool) -> Router {
        let chain = Arc::new(Mutex::new(chain));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain, mempool).with_alpha_airdrop_enabled(enabled);
        api_router(state)
    }

    async fn response_body(router: Router, body: &str) -> (StatusCode, String) {
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/alpha/airdrop")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn disabled_by_default_route_is_not_available() {
        let dir = temp_dir();
        let chain = open_chain(&dir);
        let router = router_for(chain, false);

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/alpha/airdrop")
                    .body(Body::from(r#"{"address":"aa","amount":1}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn enabled_only_with_flag_funds_address_and_persists() {
        let dir = temp_dir();
        let chain = open_chain(&dir);
        let address = "aa".repeat(32);
        let router = router_for(chain, true);

        let (status, body) = response_body(
            router,
            &format!(r#"{{"address":"{}","amount":5000}}"#, address),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("\"status\":\"accepted\""));
        assert!(body.contains("\"scope\":\"alpha_dev_only\""));
        assert!(body.contains(&format!("\"address\":\"{}\"", address)));
        assert!(body.contains("\"amount\":5000"));
        assert!(body.contains("\"balance\":5000"));
        assert!(body.contains("\"canonical_tip_height\":0"));

        let restarted = open_chain(&dir);
        assert_eq!(restarted.balance_of(&address), 5000);
        assert_eq!(restarted.nonce_of(&address), 0);
        assert_eq!(restarted.cached_state_root.as_ref().unwrap().0, 0);
    }

    #[tokio::test]
    async fn malformed_address_and_zero_amount_are_rejected() {
        let dir = temp_dir();
        let chain = open_chain(&dir);
        let router = router_for(chain, true);

        let (status, body) =
            response_body(router.clone(), r#"{"address":"not-hex","amount":1}"#).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("invalid_address"));

        let (status, body) = response_body(
            router,
            &format!(r#"{{"address":"{}","amount":0}}"#, "aa".repeat(32)),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("zero_amount"));
    }

    #[tokio::test]
    async fn malformed_request_returns_structured_response() {
        let dir = temp_dir();
        let chain = open_chain(&dir);
        let router = router_for(chain, true);

        let (status, body) = response_body(router, "{").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, "{\"status\":\"malformed_request\",\"scope\":\"alpha_dev_only\",\"error\":{\"code\":\"malformed_request\",\"message\":\"request body must be a JSON object with address and amount\"}}" );
    }
}

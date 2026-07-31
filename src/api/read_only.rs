use axum::{
    extract::{Path, State},
    Json,
};

use crate::api::state::{
    AccountBalanceSnapshot, AccountNonceSnapshot, NodeApiState, TransactionLookupSnapshot,
};

/// GET /balance/:address
pub(crate) async fn get_balance(
    Path(address): Path<String>,
    State(state): State<NodeApiState>,
) -> Json<AccountBalanceSnapshot> {
    Json(state.balance_snapshot(&address).await)
}

/// GET /nonce/:address
pub(crate) async fn get_nonce(
    Path(address): Path<String>,
    State(state): State<NodeApiState>,
) -> Json<AccountNonceSnapshot> {
    Json(state.nonce_snapshot(&address).await)
}

/// GET /transaction/:txid
pub(crate) async fn get_transaction(
    Path(txid): Path<String>,
    State(state): State<NodeApiState>,
) -> Json<TransactionLookupSnapshot> {
    Json(state.transaction_snapshot(&txid).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{self, Body},
        http::{Request, StatusCode},
        Router,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use serde::de::DeserializeOwned;
    use std::{path::Path, sync::Arc};
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    use crate::{
        api::routes::api_router,
        chain::{
            accept::tests_helpers::coinbase_tx,
            snapshots::save_snapshot,
            state::ChainState,
            storage::{persist_tip, store_block, store_height_index},
        },
        config::{constants::TARGET_BLOCK_TIME, settings::Settings},
        types::{
            transaction::{
                canonical_tx_id, canonical_unsigned_payload, simulate_tx_execution,
                CashTransferArgs, TxExecutionState, MIN_CASH_TRANSFER_FEE_LIMIT,
            },
            Block, BlockHeader, Tx,
        },
    };

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn signing_address(seed: u8) -> String {
        hex::encode(signing_key(seed).verifying_key().to_bytes())
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn sign_tx(mut tx: Tx, seed: u8) -> Tx {
        let sk = signing_key(seed);
        tx.sender_pubkey = hex::encode(sk.verifying_key().to_bytes());
        tx.sig.clear();
        tx.sig = hex::encode(sk.sign(&canonical_unsigned_payload(&tx)).to_bytes());
        tx
    }

    fn transfer_tx(seed: u8, nonce: u64, to: &str, amount: u128, tip: u64, fee_limit: u64) -> Tx {
        sign_tx(
            Tx {
                nonce,
                sender_pubkey: String::new(),
                module: "cash".to_string(),
                method: "transfer".to_string(),
                args: transfer_args(to, amount),
                tip,
                fee_limit,
                sig: String::new(),
            },
            seed,
        )
    }

    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn settings_for(dir: &Path) -> Settings {
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
            alpha_airdrop_enabled: false,
            mining_threads: 0,
            seed_peers: vec![],
        }
    }

    async fn open_node(dir: &TempDir) -> ChainState {
        let settings = settings_for(dir.path());
        let mut chain = ChainState::open_with_genesis(&settings.data_dir).unwrap();
        crate::node::bootstrap::bootstrap_chain(&mut chain, &settings).unwrap();
        let genesis_hash = chain.block_at(0).unwrap().hash().to_string();
        store_height_index(&chain, 0, &genesis_hash).unwrap();
        let current_height = chain.current_height();
        let _ = crate::chain::snapshots::restore_latest_snapshot(&mut chain, current_height);
        chain
    }

    fn canonical_test_block(chain: &mut ChainState, tx: Tx) -> Block {
        let parent_hash = chain.tip_hash();
        let height = chain.current_height() + 1;
        let timestamp = chain
            .block_at(chain.current_height())
            .unwrap()
            .header
            .timestamp
            + TARGET_BLOCK_TIME;

        let mut exec_state = TxExecutionState::from_balances_and_nonces(
            chain.balances.clone(),
            chain.nonces.clone(),
        );
        simulate_tx_execution(&mut exec_state, &tx).unwrap();
        chain.balances = exec_state.balances;
        chain.nonces = exec_state.nonces;

        let state_root = "11".repeat(32);
        let mut block = Block {
            header: BlockHeader {
                parent_hash,
                number: height,
                timestamp,
                difficulty: chain.difficulty,
                nonce: 0,
                pow_hash: format!("{:064x}", height + 0xabc),
                state_root: state_root.clone(),
                tx_root: String::new(),
                miner: signing_address(99),
            },
            txs: vec![coinbase_tx(height), tx],
            weight: 0,
        };
        block.header.tx_root = block.compute_tx_root();
        block
    }

    fn persist_canonical_block(chain: &mut ChainState, block: &Block) {
        store_block(chain, block).unwrap();
        store_height_index(chain, block.height(), block.hash()).unwrap();
        let cumulative = chain
            .cumulative_work
            .values()
            .next_back()
            .copied()
            .unwrap_or(0)
            + block.header.difficulty as u128;
        chain.blocks.push(block.clone());
        chain
            .canon_index
            .insert(block.hash().to_string(), block.height());
        chain
            .cumulative_work
            .insert(block.hash().to_string(), cumulative);
        chain.cached_state_root = Some((block.height(), block.header.state_root.clone()));
        persist_tip(chain).unwrap();
    }

    fn api_state(chain: Arc<Mutex<ChainState>>) -> NodeApiState {
        NodeApiState::new(chain, Arc::new(crate::mempool::Mempool::new()))
    }

    async fn router_response<T: DeserializeOwned>(router: Router, uri: String) -> (StatusCode, T) {
        let response = router
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes).unwrap();
        (status, value)
    }

    #[tokio::test]
    async fn balance_and_nonce_endpoints_return_canonical_state() {
        let dir = temp_dir();
        let mut chain = open_node(&dir).await;
        let account = signing_address(1);
        chain.credit_balance(&account, 42_000);
        chain.advance_nonce(&account, 7);

        let state = Arc::new(Mutex::new(chain));
        let router = api_router(api_state(state.clone()));

        let (balance_status, balance): (StatusCode, AccountBalanceSnapshot) =
            router_response(router.clone(), format!("/balance/{}", account)).await;
        let (nonce_status, nonce): (StatusCode, AccountNonceSnapshot) =
            router_response(router.clone(), format!("/nonce/{}", account)).await;
        let unknown = signing_address(2);
        let (_, unknown_balance): (StatusCode, AccountBalanceSnapshot) =
            router_response(router.clone(), format!("/balance/{}", unknown)).await;
        let (_, unknown_nonce): (StatusCode, AccountNonceSnapshot) =
            router_response(router.clone(), format!("/nonce/{}", unknown)).await;

        assert_eq!(balance_status, StatusCode::OK);
        assert_eq!(nonce_status, StatusCode::OK);
        assert_eq!(
            balance,
            AccountBalanceSnapshot {
                address: account.clone(),
                exists: true,
                balance: 42_000
            }
        );
        assert_eq!(
            nonce,
            AccountNonceSnapshot {
                address: account.clone(),
                exists: true,
                nonce: 7
            }
        );
        assert_eq!(
            unknown_balance,
            AccountBalanceSnapshot {
                address: unknown.clone(),
                exists: false,
                balance: 0
            }
        );
        assert_eq!(
            unknown_nonce,
            AccountNonceSnapshot {
                address: unknown,
                exists: false,
                nonce: 0
            }
        );
    }

    #[tokio::test]
    async fn read_only_endpoints_survive_restart() {
        let dir = temp_dir();
        let mut chain = open_node(&dir).await;
        let account = signing_address(3);
        chain.credit_balance(&account, 9_999);
        chain.advance_nonce(&account, 4);
        save_snapshot(&chain, chain.current_height()).unwrap();

        drop(chain);

        let mut restarted = open_node(&dir).await;
        let restarted_height = restarted.current_height();
        let restored_height =
            crate::chain::snapshots::restore_latest_snapshot(&mut restarted, restarted_height)
                .unwrap();
        assert_eq!(restored_height, 0);

        let state = Arc::new(Mutex::new(restarted));
        let router = api_router(api_state(state));
        let (balance_status, balance): (StatusCode, AccountBalanceSnapshot) =
            router_response(router.clone(), format!("/balance/{}", account)).await;
        let (nonce_status, nonce): (StatusCode, AccountNonceSnapshot) =
            router_response(router.clone(), format!("/nonce/{}", account)).await;

        assert_eq!(balance_status, StatusCode::OK);
        assert_eq!(nonce_status, StatusCode::OK);
        assert_eq!(balance.balance, 9_999);
        assert!(balance.exists);
        assert_eq!(nonce.nonce, 4);
        assert!(nonce.exists);
    }

    #[tokio::test]
    async fn transaction_lookup_returns_mined_tx_and_unknown_tx_returns_missing() {
        let dir = temp_dir();
        let mut chain = open_node(&dir).await;
        let sender = signing_address(4);
        let recipient = signing_address(5);
        chain.credit_balance(&sender, 50_000);

        let tx = transfer_tx(4, 0, &recipient, 10_000, 5, MIN_CASH_TRANSFER_FEE_LIMIT);
        let tx_id = canonical_tx_id(&tx);
        let block = canonical_test_block(&mut chain, tx.clone());
        persist_canonical_block(&mut chain, &block);
        save_snapshot(&chain, chain.current_height()).unwrap();

        let state = Arc::new(Mutex::new(chain));
        let router = api_router(api_state(state));
        let (status, lookup): (StatusCode, TransactionLookupSnapshot) =
            router_response(router.clone(), format!("/transaction/{}", tx_id)).await;
        let (_, missing): (StatusCode, TransactionLookupSnapshot) =
            router_response(router, format!("/transaction/{}", "ff".repeat(32))).await;

        assert_eq!(status, StatusCode::OK);
        assert!(lookup.found);
        assert_eq!(lookup.tx_id, tx_id);
        assert_eq!(lookup.block_height, Some(1));
        assert_eq!(lookup.tx, Some(tx));
        assert!(!missing.found);
        assert_eq!(missing.block_hash, None);
        assert_eq!(missing.tx, None);
    }
}

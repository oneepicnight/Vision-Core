#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
    use std::sync::Arc;

    use anyhow::{anyhow, Result};
    use axum::body::{self, Bytes};
    use axum::extract::State;
    use crate::api::transactions::submit_transaction_http;
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::Mutex;
    use tokio::task::JoinHandle;
    use tokio::time::{timeout, Duration};

    use crate::api::routes::api_router;
    use crate::api::state::NodeApiState;
    use crate::chain::accept::{apply_block, tests_helpers::coinbase_tx, AcceptResult};
    use crate::chain::state::ChainState;
    use crate::config::constants::{DIFFICULTY_FLOOR, TARGET_BLOCK_TIME};
    use crate::config::settings::Settings;
    use crate::mempool::Mempool;
    use crate::node::bootstrap::bootstrap_chain;
    use crate::p2p::connection::P2PConnectionManager;
    use crate::p2p::peer_manager::{PeerManager, PeerState};
    use crate::p2p::sync::{watchdog_step, SyncGuard};
    use crate::pow::visionx::{historical_block_digest, VISIONX_PARAMS};
    use crate::types::transaction::{
        canonical_tx_id, canonical_unsigned_payload, simulate_tx_execution, CashTransferArgs,
        TxExecutionState, MIN_CASH_TRANSFER_FEE_LIMIT,
    };
    use crate::types::{Block, BlockHeader, Tx};

    struct NodeHarness {
        data_dir: TempDir,
        addr: SocketAddr,
        api_addr: Option<SocketAddr>,
        api_state: Option<NodeApiState>,
        chain: Arc<Mutex<ChainState>>,
        mempool: Arc<Mempool>,
        peer_manager: Arc<PeerManager>,
        conn_mgr: Arc<P2PConnectionManager>,
        p2p_task: JoinHandle<()>,
        api_task: Option<JoinHandle<()>>,
    }

    fn fresh_port() -> SocketAddr {
        StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("free port")
            .local_addr()
            .expect("local addr")
    }

    fn node_settings(data_dir: &TempDir, addr: SocketAddr) -> Settings {
        Settings {
            data_dir: data_dir.path().display().to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            p2p_addr: addr.to_string(),
            mining_enabled: false,
            mining_threads: 0,
            seed_peers: vec![],
        }
    }

    async fn start_node(with_api: bool) -> Result<NodeHarness> {
        let data_dir = tempfile::tempdir()?;
        let addr = fresh_port();
        let settings = node_settings(&data_dir, addr);

        let mut chain_state = ChainState::open(&settings.data_dir)?;
        bootstrap_chain(&mut chain_state, &settings)?;

        let chain = Arc::new(Mutex::new(chain_state));
        let mempool = Arc::new(Mempool::new());
        let peer_manager = Arc::new(PeerManager::new());
        let conn_mgr = Arc::new(P2PConnectionManager::new(
            settings.p2p_addr.parse()?,
            chain.clone(),
            peer_manager.clone(),
        ));
        let p2p_task = {
            let conn_mgr = conn_mgr.clone();
            tokio::spawn(async move {
                if let Err(e) = conn_mgr.run_listener().await {
                    tracing::warn!("[TEST-NODE] listener exited: {}", e);
                }
            })
        };

        let (api_addr, api_task, api_state) = if with_api {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let api_addr = listener.local_addr()?;
            let state = NodeApiState::new(chain.clone(), mempool.clone())
                .with_peer_manager(peer_manager.clone());
            let api_state = Some(state.clone());
            let router = api_router(state);
            let task = tokio::spawn(async move {
                if let Err(e) = axum::serve(listener, router).await {
                    tracing::warn!("[TEST-NODE] API server exited: {}", e);
                }
            });
            (Some(api_addr), Some(task), api_state)
        } else {
            (None, None, None)
        };

        Ok(NodeHarness {
            data_dir,
            addr,
            api_addr,
            api_state,
            chain,
            mempool,
            peer_manager,
            conn_mgr,
            p2p_task,
            api_task,
        })
    }

    async fn stop_node(node: NodeHarness) {
        node.p2p_task.abort();
        let _ = node.p2p_task.await;
        if let Some(task) = node.api_task {
            task.abort();
            let _ = task.await;
        }
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn sign_tx(mut tx: Tx, seed: u8) -> Tx {
        let signing_key = signing_key(seed);
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();
        let sig = signing_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    fn transfer_tx(
        seed: u8,
        nonce: u64,
        to: &str,
        amount: u128,
        tip: u64,
        fee_limit: u64,
    ) -> Tx {
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

    fn compute_state_root_like_core(
        balances: &BTreeMap<String, u128>,
        nonces: &BTreeMap<String, u64>,
    ) -> Result<String> {
        fn is_lower_hex_byte(byte: u8) -> bool {
            matches!(byte, b'0'..=b'9' | b'a'..=b'f')
        }

        fn decode_account_key(key: &str) -> Result<[u8; 32]> {
            if key.len() != 64 {
                return Err(anyhow!("malformed key"));
            }
            let mut saw_upper = false;
            let mut saw_other = false;
            for byte in key.as_bytes() {
                if matches!(byte, b'A'..=b'F') {
                    saw_upper = true;
                } else if !is_lower_hex_byte(*byte) {
                    saw_other = true;
                }
            }
            if saw_upper {
                return Err(anyhow!("mixed case key"));
            }
            if saw_other {
                return Err(anyhow!("malformed key"));
            }
            let bytes = hex::decode(key)?;
            bytes
                .try_into()
                .map_err(|_| anyhow!("malformed key"))
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"VSTATE");
        out.extend_from_slice(&1u32.to_le_bytes());

        let mut balances: Vec<_> = balances.iter().filter(|(_, amount)| **amount != 0).collect();
        balances.sort_by(|(a, _), (b, _)| a.cmp(b));
        out.extend_from_slice(&(balances.len() as u64).to_le_bytes());
        for (key, amount) in balances {
            out.extend_from_slice(&decode_account_key(key)?);
            out.extend_from_slice(&amount.to_le_bytes());
        }

        let mut nonces: Vec<_> = nonces.iter().filter(|(_, nonce)| **nonce != 0).collect();
        nonces.sort_by(|(a, _), (b, _)| a.cmp(b));
        out.extend_from_slice(&(nonces.len() as u64).to_le_bytes());
        for (key, nonce) in nonces {
            out.extend_from_slice(&decode_account_key(key)?);
            out.extend_from_slice(&nonce.to_le_bytes());
        }

        Ok(hex::encode(blake3::hash(&out).as_bytes()))
    }

    fn build_mined_block(
        parent: &Block,
        height: u64,
        timestamp: u64,
        slot: u8,
        extra_txs: Vec<Tx>,
        balances: &BTreeMap<String, u128>,
        nonces: &BTreeMap<String, u64>,
        miner: &str,
    ) -> Result<Block> {
        let mut txs = Vec::with_capacity(1 + extra_txs.len());
        txs.push(coinbase_tx(height));
        txs.extend(extra_txs);

        let tx_root = if txs.is_empty() {
            "0".repeat(64)
        } else {
            let mut h = blake3::Hasher::new();
            for tx in &txs {
                h.update(canonical_tx_id(tx).as_bytes());
            }
            hex::encode(h.finalize().as_bytes())
        };

        let mut exec_state = TxExecutionState::from_balances_and_nonces(
            balances.clone(),
            nonces.clone(),
        );
        for tx in txs.iter().skip(1) {
            simulate_tx_execution(&mut exec_state, tx).map_err(|e| anyhow!("tx execution failed: {:?}", e))?;
        }
        let state_root = compute_state_root_like_core(&exec_state.balances, &exec_state.nonces)?;

        let mut header = BlockHeader {
            parent_hash: parent.hash().to_string(),
            number: height,
            timestamp,
            difficulty: DIFFICULTY_FLOOR,
            nonce: slot as u64,
            pow_hash: String::new(),
            state_root,
            tx_root,
            miner: miner.to_string(),
        };

        let epoch = VISIONX_PARAMS.epoch(height);
        let digest = historical_block_digest(&VISIONX_PARAMS, epoch, &header)
            .map_err(|e| anyhow!(e))?;
        header.pow_hash = hex::encode(digest);

        let weight = txs.len() as u64;
        Ok(Block {
            header,
            txs,
            weight,
        })
    }

    async fn mine_and_apply_empty_block(
        chain: &Arc<Mutex<ChainState>>,
        height: u64,
        slot: u8,
        miner: &str,
    ) -> Result<Block> {
        let block = {
            let guard = chain.lock().await;
            let tip = guard.blocks.last().expect("tip exists").clone();
            let timestamp = tip.header.timestamp + TARGET_BLOCK_TIME;
            build_mined_block(
                &tip,
                height,
                timestamp,
                slot,
                vec![],
                &guard.balances,
                &guard.nonces,
                miner,
            )?
        };

        let result = {
            let mut guard = chain.lock().await;
            apply_block(&mut guard, &block, None)
        };
        assert_eq!(result, AcceptResult::CanonExtension { height });
        Ok(block)
    }

    async fn post_json(state: NodeApiState, body: &str) -> Result<(u16, String)> {
        let response = submit_transaction_http(State(state), Bytes::from(body.to_owned())).await;
        let status = response.status().as_u16();
        let bytes = body::to_bytes(response.into_body(), usize::MAX).await?;
        Ok((status, String::from_utf8(bytes.to_vec())?))
    }

    fn mine_state_keys(sender: &str, recipient: &str) -> BTreeMap<String, u128> {
        let mut balances = BTreeMap::new();
        balances.insert(sender.to_string(), 1_000);
        balances.insert(recipient.to_string(), 0);
        balances
    }

    #[tokio::test]
    async fn two_node_local_testnet_catches_up_over_tcp() -> Result<()> {
        let miner = start_node(true).await?;
        let follower = start_node(false).await?;

        assert_ne!(miner.addr, follower.addr);
        assert_ne!(miner.data_dir.path(), follower.data_dir.path());

        let sender_key = signing_key(11);
        let sender = hex::encode(sender_key.verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(12).verifying_key().to_bytes());
        let balances = mine_state_keys(&sender, &recipient);
        {
            let mut miner_chain = miner.chain.lock().await;
            miner_chain.balances = balances.clone();
            miner_chain.nonces.insert(sender.clone(), 0);
        }
        {
            let mut follower_chain = follower.chain.lock().await;
            follower_chain.balances = balances.clone();
            follower_chain.nonces.insert(sender.clone(), 0);
        }

        let miner_genesis = {
            let guard = miner.chain.lock().await;
            guard.block_at(0).unwrap().hash().to_string()
        };
        let follower_genesis = {
            let guard = follower.chain.lock().await;
            guard.block_at(0).unwrap().hash().to_string()
        };
        assert_eq!(miner_genesis, follower_genesis);

        for height in 1..=4 {
            let _ = mine_and_apply_empty_block(
                &miner.chain,
                height,
                0xA0u8.wrapping_add(height as u8),
                "miner-a",
            )
            .await?;
        }

        let transfer = transfer_tx(11, 0, &recipient, 100, 2, MIN_CASH_TRANSFER_FEE_LIMIT);
        let transfer_json = serde_json::to_string(&transfer)?;
        let transfer_id = canonical_tx_id(&transfer);

        let api_state = miner.api_state.clone().ok_or_else(|| anyhow!("missing API state"))?;
        let (status, body) = post_json(api_state, &transfer_json).await?;
        assert_eq!(status, 200);
        assert!(body.contains("\"status\":\"accepted\""));
        assert!(body.contains(&transfer_id));

        let (tip, balances, nonces, mempool_txs) = {
            let guard = miner.chain.lock().await;
            (
                guard.blocks.last().unwrap().clone(),
                guard.balances.clone(),
                guard.nonces.clone(),
                miner.mempool.select_for_block(200),
            )
        };
        let block = build_mined_block(
            &tip,
            5,
            tip.header.timestamp + TARGET_BLOCK_TIME,
            0xFE,
            mempool_txs,
            &balances,
            &nonces,
            "miner-a",
        )?;
        let result = {
            let mut guard = miner.chain.lock().await;
            apply_block(&mut guard, &block, None)
        };
        assert_eq!(result, AcceptResult::CanonExtension { height: 5 });
        miner.mempool.remove_confirmed(&[transfer_id.clone()]);
        let txs = block.txs;
        assert_eq!(txs.len(), 2);
        let follower_peer = miner.addr.to_string();
        follower.peer_manager.upsert(&follower_peer, true);
        follower.peer_manager.set_state(&follower_peer, PeerState::Connected);
        follower.peer_manager.note_peer_height(&follower_peer, 5, false);

        let mut guard = SyncGuard::new();
        timeout(
            Duration::from_secs(10),
            watchdog_step(
                follower.conn_mgr.as_ref(),
                &follower.chain,
                follower.peer_manager.as_ref(),
                &mut guard,
            ),
        )
        .await
        .unwrap()?;

        let sender_balance = 897u128;
        let recipient_balance = 100u128;
        let sender_nonce = 1u64;

        let miner_snapshot = {
            let guard = miner.chain.lock().await;
            let tip = guard.blocks.last().unwrap();
            (
                guard.current_height(),
                guard.tip_hash(),
                tip.header.tx_root.clone(),
                tip.header.state_root.clone(),
                guard.balance_of(&sender),
                guard.balance_of(&recipient),
                guard.nonce_of(&sender),
            )
        };
        let follower_snapshot = {
            let guard = follower.chain.lock().await;
            let tip = guard.blocks.last().unwrap();
            (
                guard.current_height(),
                guard.tip_hash(),
                tip.header.tx_root.clone(),
                tip.header.state_root.clone(),
                guard.balance_of(&sender),
                guard.balance_of(&recipient),
                guard.nonce_of(&sender),
            )
        };

        assert_eq!(miner_snapshot.0, 5);
        assert_eq!(follower_snapshot.0, 5);
        assert_eq!(miner_snapshot, follower_snapshot);
        assert_eq!(miner_snapshot.4, sender_balance);
        assert_eq!(miner_snapshot.5, recipient_balance);
        assert_eq!(miner_snapshot.6, sender_nonce);

        stop_node(miner).await;
        stop_node(follower).await;
        Ok(())
    }
}



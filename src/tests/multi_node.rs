#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
    use std::sync::Arc;
    use std::path::{Path, PathBuf};

    use anyhow::{anyhow, Result};
    use serde::{Deserialize, Serialize};
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
    use crate::node::bootstrap::initialize_chain_state;
    use crate::p2p::connection::P2PConnectionManager;
    use crate::p2p::peer_manager::{PeerManager, PeerState};
    use crate::p2p::sync::{watchdog_step, SyncGuard};
    use crate::pow::visionx::{historical_block_digest, VISIONX_PARAMS};
    use crate::types::transaction::{
        canonical_tx_id, canonical_unsigned_payload, simulate_tx_execution, CashTransferArgs,
        TxExecutionState, MIN_CASH_TRANSFER_FEE_LIMIT,
    };
    use crate::types::{Block, BlockHeader, Tx};

    const ZERO_MINER: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    struct NodeHarness {
        data_dir: PathBuf,
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

    fn node_settings(data_dir: &Path, addr: SocketAddr) -> Settings {
        Settings {
            data_dir: data_dir.display().to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            p2p_addr: addr.to_string(),
            mining_enabled: false,
            mining_threads: 0,
            alpha_airdrop_enabled: false,
            miner_address: "0".repeat(64),
            seed_peers: vec![],
        }
    }

    async fn start_node(with_api: bool) -> Result<NodeHarness> {
        let data_dir = tempfile::tempdir()?.into_path();
        start_node_in_dir(data_dir, with_api).await
    }

    async fn start_node_in_dir(data_dir: PathBuf, with_api: bool) -> Result<NodeHarness> {
        let addr = fresh_port();
        let settings = node_settings(&data_dir, addr);

        let chain_state = initialize_chain_state(&settings)?;
        let genesis_hash = chain_state.block_at(0).unwrap().hash().to_string();
        assert_eq!(crate::chain::storage::load_height_index(&chain_state, 0)?.as_deref(), Some(genesis_hash.as_str()));

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

    async fn start_node_from_existing_dir(data_dir: PathBuf, with_api: bool) -> Result<NodeHarness> {
        start_node_in_dir(data_dir, with_api).await
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
        if height != 0 {
            crate::chain::accept::apply_coinbase_reward(&mut exec_state, miner, height)
                .map_err(|e| anyhow!("coinbase reward failed: {:?}", e))?;
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
            let result = apply_block(&mut guard, &block, None);
            if matches!(result, AcceptResult::CanonExtension { .. }) {
                guard.refresh_cached_state_root_from_tip();
            }
            result
        };
        assert_eq!(result, AcceptResult::CanonExtension { height });
        {
            let guard = chain.lock().await;
            crate::chain::storage::store_height_index(&guard, height, &block.hash())?;
        }

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

    async fn sync_node_to_peer(node: &NodeHarness, peer_addr: &str, reported_height: u64) -> Result<()> {
        node.peer_manager.upsert(peer_addr, true);
        node.peer_manager.set_state(peer_addr, PeerState::Connected);
        node.peer_manager.note_peer_height(peer_addr, reported_height, false);

        let mut guard = SyncGuard::new();
        timeout(
            Duration::from_secs(15),
            watchdog_step(
                node.conn_mgr.as_ref(),
                &node.chain,
                node.peer_manager.as_ref(),
                &mut guard,
            None,
            ),
        )
        .await
        .unwrap()?;

        Ok(())
    }


    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TxRecordArtifact {
        tx_id: String,
        submitted_at: String,
    }

    fn persist_tx_records(path: &Path, records: &[TxRecordArtifact]) -> Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(records)?)?;
        Ok(())
    }

    async fn wait_for_progress_aware_convergence<F, Fut>(
        label: &str,
        overall_timeout: Duration,
        no_progress_timeout: Duration,
        interval: Duration,
        mut poll: F,
    ) -> Result<()>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<(bool, u64)>>,
    {
        let mut overall_deadline = tokio::time::Instant::now() + overall_timeout;
        let mut stall_deadline = tokio::time::Instant::now() + no_progress_timeout;
        let mut last_progress = 0u64;
        let mut saw_progress = false;

        loop {
            let now = tokio::time::Instant::now();
            if now >= stall_deadline {
                return Err(anyhow!("timeout waiting for {}: no progress before deadline", label));
            }
            if now >= overall_deadline {
                return Err(anyhow!("timeout waiting for {}: overall deadline reached", label));
            }

            let (done, progress) = poll().await?;
            if done {
                return Ok(());
            }
            if !saw_progress || progress > last_progress {
                saw_progress = true;
                last_progress = progress;
                overall_deadline = tokio::time::Instant::now() + overall_timeout;
                stall_deadline = tokio::time::Instant::now() + no_progress_timeout;
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn node_snapshot(node: &NodeHarness, sender: &str, recipient: &str) -> (u64, String, String, String, u128, u128, u64) {
        let guard = node.chain.lock().await;
        let tip = guard.blocks.last().unwrap();
        (
            guard.current_height(),
            guard.tip_hash(),
            tip.header.tx_root.clone(),
            tip.header.state_root.clone(),
            guard.balance_of(sender),
            guard.balance_of(recipient),
            guard.nonce_of(sender),
        )
    }

    #[test]
    fn reward_only_height_one_block_matches_independent_follower() -> Result<()> {
        let db_miner = sled::Config::new().temporary(true).open()?;
        let db_follower = sled::Config::new().temporary(true).open()?;
        let mut miner = ChainState::empty(db_miner);
        let mut follower = ChainState::empty(db_follower);
        let genesis = crate::genesis::genesis_block();
        apply_block(&mut miner, &genesis, None);
        apply_block(&mut follower, &genesis, None);

        let before_balances = miner.balances.clone();
        let before_nonces = miner.nonces.clone();
        let tip = miner.blocks.last().unwrap().clone();
        let block = build_mined_block(
            &tip,
            1,
            tip.header.timestamp + TARGET_BLOCK_TIME,
            0xAA,
            vec![],
            &miner.balances,
            &miner.nonces,
            ZERO_MINER,
        )?;

        assert_eq!(miner.balances, before_balances);
        assert_eq!(miner.nonces, before_nonces);

        assert_eq!(apply_block(&mut miner, &block, None), AcceptResult::CanonExtension { height: 1 });
        assert_eq!(apply_block(&mut follower, &block, None), AcceptResult::CanonExtension { height: 1 });

        let miner_tip = miner.blocks.last().unwrap();
        let follower_tip = follower.blocks.last().unwrap();
        assert_eq!(miner_tip.header.state_root, follower_tip.header.state_root);
        assert_eq!(miner.balance_of(ZERO_MINER), crate::miner::job::block_reward(1));
        assert_eq!(follower.balance_of(ZERO_MINER), crate::miner::job::block_reward(1));
        Ok(())
    }
    #[tokio::test]
    async fn two_node_local_testnet_catches_up_over_tcp() -> Result<()> {
        let miner = start_node(true).await?;
        let follower = start_node(false).await?;

        assert_ne!(miner.addr, follower.addr);
        assert_ne!(miner.data_dir.as_path(), follower.data_dir.as_path());

        let sender_key = signing_key(11);
        let sender = hex::encode(sender_key.verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(12).verifying_key().to_bytes());
        let balances = BTreeMap::from([(sender.clone(), 0u128), (recipient.clone(), 0u128)]);
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

        let funding_block = {
            let guard = miner.chain.lock().await;
            let tip = guard.blocks.last().unwrap().clone();
            build_mined_block(
                &tip,
                1,
                tip.header.timestamp + TARGET_BLOCK_TIME,
                0xA1,
                vec![],
                &guard.balances,
                &guard.nonces,
                &sender,
            )?
        };
        {
            let mut guard = miner.chain.lock().await;
            apply_block(&mut guard, &funding_block, None)
        };
        {
            let guard = miner.chain.lock().await;
            crate::chain::storage::store_height_index(&guard, 1, &funding_block.hash())?;
        }

        for height in 2..=4 {
            let _ = mine_and_apply_empty_block(
                &miner.chain,
                height,
                0xA0u8.wrapping_add(height as u8),
                ZERO_MINER,
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
            ZERO_MINER,
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
            None,
            ),
        )
        .await
        .unwrap()?;

        let mined_reward = crate::miner::job::block_reward(1);
        let sender_balance = mined_reward - 100u128 - 1u128 - 2u128;
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

    #[tokio::test]
    async fn restart_persistence_restores_chain_state_and_catches_up() -> Result<()> {
        let miner = start_node(true).await?;
        let live_peer = start_node(false).await?;

        assert_ne!(miner.addr, live_peer.addr);
        assert_ne!(miner.data_dir.as_path(), live_peer.data_dir.as_path());

        let sender_key = signing_key(31);
        let sender = hex::encode(sender_key.verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(32).verifying_key().to_bytes());
        let balances = mine_state_keys(&sender, &recipient);
        {
            let mut miner_chain = miner.chain.lock().await;
            miner_chain.balances = balances.clone();
            miner_chain.nonces.insert(sender.clone(), 0);
        }
        {
            let mut peer_chain = live_peer.chain.lock().await;
            peer_chain.balances = balances.clone();
            peer_chain.nonces.insert(sender.clone(), 0);
        }

        for height in 1..=31 {
            let _ = mine_and_apply_empty_block(
                &miner.chain,
                height,
                0xD0u8.wrapping_add(height as u8),
                ZERO_MINER,
            )
            .await?;
        }

        let transfer = transfer_tx(31, 0, &recipient, 100, 2, MIN_CASH_TRANSFER_FEE_LIMIT);
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
            32,
            tip.header.timestamp + TARGET_BLOCK_TIME,
            0xDD,
            mempool_txs,
            &balances,
            &nonces,
            ZERO_MINER,
        )?;
        let result = {
            let mut guard = miner.chain.lock().await;
            apply_block(&mut guard, &block, None)
        };
        assert_eq!(result, AcceptResult::CanonExtension { height: 32 });
        {
            let guard = miner.chain.lock().await;
            crate::chain::storage::store_height_index(&guard, 32, &block.hash())?;
        }

        miner.mempool.remove_confirmed(&[transfer_id.clone()]);

        let expected_snapshot = node_snapshot(&miner, &sender, &recipient).await;
        assert_eq!(expected_snapshot.4, 897u128);
        assert_eq!(expected_snapshot.5, 100u128);
        assert_eq!(expected_snapshot.6, 1u64);

        let live_peer_addr = live_peer.addr.to_string();
        sync_node_to_peer(&live_peer, &miner.addr.to_string(), 32).await?;
        let live_peer_snapshot_32 = node_snapshot(&live_peer, &sender, &recipient).await;
        assert_eq!(live_peer_snapshot_32, expected_snapshot);


        let restart_data_dir = miner.data_dir.clone();
        stop_node(miner).await;

        let restarted = start_node_from_existing_dir(restart_data_dir, false).await?;
        let restart_snapshot = node_snapshot(&restarted, &sender, &recipient).await;
        assert_eq!(restart_snapshot, expected_snapshot);
        {
            let guard = restarted.chain.lock().await;
            assert_eq!(
                crate::chain::storage::load_height_index(&guard, 0)?.as_deref(),
                Some(crate::genesis::GENESIS_HASH)
            );
            assert_eq!(
                crate::chain::storage::load_height_index(&guard, 32)?.as_deref(),
                Some(restart_snapshot.1.as_str())
            );
        }

        for height in 33..=40 {
            let _ = mine_and_apply_empty_block(
                &live_peer.chain,
                height,
                0xE0u8.wrapping_add(height as u8),
                ZERO_MINER,
            )
            .await?;
        }

        sync_node_to_peer(&restarted, &live_peer_addr, 40).await?;

        let restarted_snapshot = node_snapshot(&restarted, &sender, &recipient).await;
        let live_peer_snapshot = node_snapshot(&live_peer, &sender, &recipient).await;
        assert_eq!(restarted_snapshot, live_peer_snapshot);
        assert_eq!(restarted_snapshot.0, 40);
        assert_eq!(restarted_snapshot.4, 897u128);
        assert_eq!(restarted_snapshot.5, 100u128);

        stop_node(restarted).await;
        stop_node(live_peer).await;
        Ok(())
    }

    #[test]
    fn tx_ids_remain_in_artifacts_after_later_failure() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("tx-records.json");
        let first = TxRecordArtifact {
            tx_id: "aa".repeat(32),
            submitted_at: "2026-07-13T00:00:00Z".to_string(),
        };
        persist_tx_records(&path, &[first.clone()])?;

        let simulated_late_failure = Err::<(), _>(anyhow!("later stage failed after submission was recorded"));
        assert!(simulated_late_failure.is_err());

        let persisted = std::fs::read_to_string(&path)?;
        assert!(persisted.contains(&first.tx_id));
        Ok(())
    }

    #[tokio::test]
    async fn progress_aware_convergence_extends_deadline_when_progress_continues() -> Result<()> {
        let progress = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicU64::new(0));
        let progress_task = progress.clone();
        let done_task = done.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            progress_task.store(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(30)).await;
            progress_task.store(2, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(30)).await;
            progress_task.store(3, Ordering::SeqCst);
            done_task.store(1, Ordering::SeqCst);
        });

        wait_for_progress_aware_convergence(
            "progress-aware convergence",
            Duration::from_millis(300),
            Duration::from_millis(120),
            Duration::from_millis(10),
            || {
                let progress = progress.load(Ordering::SeqCst);
                let done = done.load(Ordering::SeqCst) != 0;
                async move { Ok((done, progress)) }
            },
        )
        .await?;

        assert_eq!(done.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn no_progress_timeout_still_fails_a_stalled_follower() -> Result<()> {
        let err = wait_for_progress_aware_convergence(
            "stalled follower",
            Duration::from_millis(120),
            Duration::from_millis(40),
            Duration::from_millis(5),
            || async { Ok((false, 0)) },
        )
        .await
        .expect_err("stalled follower should time out");

        assert!(err.to_string().contains("no progress before deadline"));
        Ok(())
    }

    #[tokio::test]
    async fn cached_state_root_height_tracks_canonical_blocks_in_status_snapshot() -> Result<()> {
        let node = start_node(true).await?;
        let tip = mine_and_apply_empty_block(&node.chain, 1, 0xA5, ZERO_MINER).await?;
        let state = node.api_state.clone().ok_or_else(|| anyhow!("missing API state"))?;
        let snapshot = state.status_snapshot().await;

        assert_eq!(snapshot.cached_state_root_height, Some(1));
        assert_eq!(snapshot.cached_state_root.as_deref(), Some(tip.header.state_root.as_str()));

        stop_node(node).await;
        Ok(())
    }

    #[tokio::test]
    async fn three_node_local_testnet_converges_with_reconnect() -> Result<()> {
        let miner = start_node(true).await?;
        let follower_b = start_node(false).await?;
        let follower_c = start_node(false).await?;

        assert_ne!(miner.addr, follower_b.addr);
        assert_ne!(miner.addr, follower_c.addr);
        assert_ne!(follower_b.addr, follower_c.addr);
        assert_ne!(miner.data_dir.as_path(), follower_b.data_dir.as_path());
        assert_ne!(miner.data_dir.as_path(), follower_c.data_dir.as_path());
        assert_ne!(follower_b.data_dir.as_path(), follower_c.data_dir.as_path());

        let sender_key = signing_key(21);
        let sender = hex::encode(sender_key.verifying_key().to_bytes());
        let recipient = hex::encode(signing_key(22).verifying_key().to_bytes());
        let balances = mine_state_keys(&sender, &recipient);
        for node in [&miner, &follower_b, &follower_c] {
            let mut chain = node.chain.lock().await;
            chain.balances = balances.clone();
            chain.nonces.insert(sender.clone(), 0);
        }

        let miner_genesis = { miner.chain.lock().await.block_at(0).unwrap().hash().to_string() };
        let follower_b_genesis = { follower_b.chain.lock().await.block_at(0).unwrap().hash().to_string() };
        let follower_c_genesis = { follower_c.chain.lock().await.block_at(0).unwrap().hash().to_string() };
        assert_eq!(miner_genesis, follower_b_genesis);
        assert_eq!(miner_genesis, follower_c_genesis);

        for height in 1..=4 {
            let _ = mine_and_apply_empty_block(
                &miner.chain,
                height,
                0xB0u8.wrapping_add(height as u8),
                ZERO_MINER,
            )
            .await?;
        }

        let transfer = transfer_tx(21, 0, &recipient, 100, 2, MIN_CASH_TRANSFER_FEE_LIMIT);
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
            ZERO_MINER,
        )?;
        let result = {
            let mut guard = miner.chain.lock().await;
            apply_block(&mut guard, &block, None)
        };
        assert_eq!(result, AcceptResult::CanonExtension { height: 5 });
        miner.mempool.remove_confirmed(&[transfer_id.clone()]);
        assert_eq!(block.txs.len(), 2);

        let miner_peer = miner.addr.to_string();
        sync_node_to_peer(&follower_b, &miner_peer, 5).await?;
        sync_node_to_peer(&follower_c, &miner_peer, 5).await?;

        let miner_snapshot_5 = node_snapshot(&miner, &sender, &recipient).await;
        let follower_b_snapshot_5 = node_snapshot(&follower_b, &sender, &recipient).await;
        let follower_c_snapshot_5 = node_snapshot(&follower_c, &sender, &recipient).await;
        assert_eq!(miner_snapshot_5, follower_b_snapshot_5);
        assert_eq!(miner_snapshot_5, follower_c_snapshot_5);
        assert_eq!(miner_snapshot_5.0, 5);
        assert_eq!(miner_snapshot_5.4, 897u128);
        assert_eq!(miner_snapshot_5.5, 100u128);
        assert_eq!(miner_snapshot_5.6, 1u64);

        follower_b.peer_manager.set_state(&miner_peer, PeerState::Disconnected);

        for height in 6..=10 {
            let _ = mine_and_apply_empty_block(
                &miner.chain,
                height,
                0xC0u8.wrapping_add(height as u8),
                ZERO_MINER,
            )
            .await?;
        }

        sync_node_to_peer(&follower_c, &miner_peer, 10).await?;
        sync_node_to_peer(&follower_b, &miner_peer, 10).await?;

        let miner_snapshot = node_snapshot(&miner, &sender, &recipient).await;
        let follower_b_snapshot = node_snapshot(&follower_b, &sender, &recipient).await;
        let follower_c_snapshot = node_snapshot(&follower_c, &sender, &recipient).await;

        assert_eq!(miner_snapshot.0, 10);
        assert_eq!(follower_b_snapshot.0, 10);
        assert_eq!(follower_c_snapshot.0, 10);
        assert_eq!(miner_snapshot, follower_b_snapshot);
        assert_eq!(miner_snapshot, follower_c_snapshot);
        assert_eq!(miner_snapshot.4, 897u128);
        assert_eq!(miner_snapshot.5, 100u128);
        assert_eq!(miner_snapshot.6, 1u64);

        stop_node(miner).await;
        stop_node(follower_b).await;
        stop_node(follower_c).await;
        Ok(())
    }
}












#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
    use std::sync::Arc;

    use anyhow::Result;
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tokio::task::JoinHandle;
    use tokio::time::{timeout, Duration};

    use crate::chain::accept::{apply_block, tests_helpers::make_test_block, AcceptResult};
    use crate::node::bootstrap::bootstrap_chain;
    use crate::chain::state::ChainState;
    use crate::config::constants::TARGET_BLOCK_TIME;
    use crate::config::settings::Settings;
    use crate::genesis::genesis_block;
    use crate::p2p::connection::P2PConnectionManager;
    use crate::p2p::peer_manager::{PeerManager, PeerState};
    use crate::p2p::sync::{watchdog_step, SyncGuard};

    struct NodeHarness {
        data_dir: TempDir,
        addr: SocketAddr,
        chain: Arc<Mutex<ChainState>>,
        peer_manager: Arc<PeerManager>,
        conn_mgr: Arc<P2PConnectionManager>,
        listener: JoinHandle<()>,
    }

    fn fresh_p2p_addr() -> SocketAddr {
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

    fn start_node() -> Result<NodeHarness> {
        let data_dir = tempfile::tempdir();
        let data_dir = data_dir?;
        let addr = fresh_p2p_addr();
        let settings = node_settings(&data_dir, addr);

        let mut chain_state = ChainState::open(&settings.data_dir)?;
        bootstrap_chain(&mut chain_state, &settings)?;

        let chain = Arc::new(Mutex::new(chain_state));
        let peer_manager = Arc::new(PeerManager::new());
        let conn_mgr = Arc::new(P2PConnectionManager::new(
            settings.p2p_addr.parse()?,
            chain.clone(),
            peer_manager.clone(),
        ));
        let listener = {
            let conn_mgr = conn_mgr.clone();
            tokio::spawn(async move {
                if let Err(e) = conn_mgr.run_listener().await {
                    tracing::warn!("[TEST-NODE] listener exited: {}", e);
                }
            })
        };

        Ok(NodeHarness {
            data_dir,
            addr,
            chain,
            peer_manager,
            conn_mgr,
            listener,
        })
    }

    async fn stop_node(node: NodeHarness) {
        node.listener.abort();
        let _ = node.listener.await;
    }

    async fn mine_blocks(chain: &Arc<Mutex<ChainState>>, count: u64) -> Result<Vec<String>> {
        let genesis = genesis_block();
        let mut parent_hash = genesis.hash().to_string();
        let mut timestamp = genesis.header.timestamp;
        let mut tip_hashes = Vec::new();

        for height in 1..=count {
            timestamp += TARGET_BLOCK_TIME;
            let block = make_test_block(
                &parent_hash,
                height,
                timestamp,
                0xA0u8.wrapping_add(height as u8),
            );
            let result = {
                let mut guard = chain.lock().await;
                apply_block(&mut guard, &block, None)
            };
            assert_eq!(result, AcceptResult::CanonExtension { height });
            parent_hash = block.hash().to_string();
            tip_hashes.push(parent_hash.clone());
        }

        Ok(tip_hashes)
    }

    #[tokio::test]
    async fn two_node_local_testnet_catches_up_over_tcp() -> Result<()> {
        let miner = start_node()?;
        let follower = start_node()?;

        assert_ne!(miner.addr, follower.addr);
        assert_ne!(miner.data_dir.path(), follower.data_dir.path());

        let miner_genesis = {
            let guard = miner.chain.lock().await;
            guard.block_at(0).unwrap().hash().to_string()
        };
        let follower_genesis = {
            let guard = follower.chain.lock().await;
            guard.block_at(0).unwrap().hash().to_string()
        };
        assert_eq!(miner_genesis, follower_genesis);

        let mined_hashes = mine_blocks(&miner.chain, 5).await?;
        let mined_tip = mined_hashes.last().cloned().unwrap();
        {
            let guard = miner.chain.lock().await;
            assert_eq!(guard.current_height(), 5);
            assert_eq!(guard.tip_hash(), mined_tip);
        }

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

        let miner_tip = {
            let guard = miner.chain.lock().await;
            let tip = guard.blocks.last().unwrap();
            (
                guard.current_height(),
                guard.tip_hash(),
                tip.header.tx_root.clone(),
                tip.header.state_root.clone(),
            )
        };
        let follower_tip = {
            let guard = follower.chain.lock().await;
            let tip = guard.blocks.last().unwrap();
            (
                guard.current_height(),
                guard.tip_hash(),
                tip.header.tx_root.clone(),
                tip.header.state_root.clone(),
            )
        };

        assert_eq!(miner_tip, follower_tip);
        assert_eq!(follower_tip.0, 5);

        stop_node(miner).await;
        stop_node(follower).await;
        Ok(())
    }
}

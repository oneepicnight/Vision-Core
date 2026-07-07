use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::api::transactions::{submit_transaction as submit_transaction_service, TransactionSubmissionResult};
use crate::chain::ChainState;
use crate::mempool::Mempool;
use crate::miner::MinerManager;
use crate::p2p::peer_manager::PeerManager;
use crate::types::Tx;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MiningStatusSnapshot {
    pub available: bool,
    pub active: bool,
    pub blocks_found: u64,
}

impl MiningStatusSnapshot {
    fn unavailable() -> Self {
        Self {
            available: false,
            active: false,
            blocks_found: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct NodeStatusSnapshot {
    pub version: &'static str,
    pub canonical_tip_height: u64,
    pub canonical_tip_hash: String,
    pub cached_state_root_height: Option<u64>,
    pub cached_state_root: Option<String>,
    pub mempool_size: usize,
    pub peer_count: usize,
    pub mining: MiningStatusSnapshot,
}

#[derive(Clone)]
pub(crate) struct NodeApiState {
    chain: Arc<Mutex<ChainState>>,
    mempool: Arc<Mempool>,
    peer_manager: Option<Arc<PeerManager>>,
    miner_manager: Option<Arc<MinerManager>>,
}

impl NodeApiState {
    pub(crate) fn new(chain: Arc<Mutex<ChainState>>, mempool: Arc<Mempool>) -> Self {
        Self {
            chain,
            mempool,
            peer_manager: None,
            miner_manager: None,
        }
    }

    pub(crate) fn with_peer_manager(mut self, peer_manager: Arc<PeerManager>) -> Self {
        self.peer_manager = Some(peer_manager);
        self
    }

    pub(crate) fn with_miner_manager(mut self, miner_manager: Arc<MinerManager>) -> Self {
        self.miner_manager = Some(miner_manager);
        self
    }

    pub(crate) async fn submit_transaction(&self, tx: Tx) -> TransactionSubmissionResult {
        let chain = self.chain.lock().await;
        submit_transaction_service(&chain, &self.mempool, tx)
    }

    pub(crate) async fn status_snapshot(&self) -> NodeStatusSnapshot {
        let (canonical_tip_height, canonical_tip_hash, cached_state_root_height, cached_state_root) = {
            let chain = self.chain.lock().await;
            let (cached_state_root_height, cached_state_root) = chain
                .cached_state_root
                .as_ref()
                .map(|(height, root)| (Some(*height), Some(root.clone())))
                .unwrap_or((None, None));

            (
                chain.current_height(),
                chain.tip_hash(),
                cached_state_root_height,
                cached_state_root,
            )
        };

        let mempool_size = self.mempool.len();
        let peer_count = self
            .peer_manager
            .as_ref()
            .map(|peers| peers.connected_count())
            .unwrap_or(0);
        let mining = self
            .miner_manager
            .as_ref()
            .map(|miner| {
                let stats = miner.stats();
                MiningStatusSnapshot {
                    available: true,
                    active: miner.is_mining(),
                    blocks_found: stats.blocks_found,
                }
            })
            .unwrap_or_else(MiningStatusSnapshot::unavailable);

        NodeStatusSnapshot {
            version: crate::config::constants::NODE_VERSION,
            canonical_tip_height,
            canonical_tip_hash,
            cached_state_root_height,
            cached_state_root,
            mempool_size,
            peer_count,
            mining,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::accept::{apply_block, tests_helpers::make_test_block};
    use crate::config::constants::TARGET_BLOCK_TIME;
    use crate::genesis::genesis_block;
    use crate::p2p::peer_manager::PeerState;
    use crate::pow::visionx::VisionXParams;
    use crate::types::Tx;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn placeholder_tx(nonce: u64) -> Tx {
        Tx {
            nonce,
            sender_pubkey: "aa".repeat(32),
            module: "cash".to_string(),
            method: "transfer".to_string(),
            args: vec![],
            tip: 0,
            fee_limit: 0,
            sig: "11".repeat(64),
        }
    }

    #[tokio::test]
    async fn status_snapshot_reports_real_read_only_node_state() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let peer_manager = Arc::new(PeerManager::new());
        let miner_manager = Arc::new(MinerManager::new(VisionXParams::default()));

        let expected_tip_hash;
        let expected_state_root;
        {
            let mut chain_guard = chain.lock().await;
            let genesis = genesis_block();
            apply_block(&mut chain_guard, &genesis, None);
            let block = make_test_block(
                genesis.hash(),
                1,
                genesis.header.timestamp + TARGET_BLOCK_TIME,
                0xAA,
            );
            expected_tip_hash = block.hash().to_string();
            expected_state_root = block.header.state_root.clone();
            apply_block(&mut chain_guard, &block, None);
            chain_guard.cached_state_root = Some((1, expected_state_root.clone()));
        }

        assert!(mempool.insert(placeholder_tx(0)));
        assert!(mempool.insert(placeholder_tx(1)));
        peer_manager.upsert("127.0.0.1:9001", true);
        peer_manager.upsert("127.0.0.1:9002", false);
        peer_manager.set_state("127.0.0.1:9001", PeerState::Connected);
        peer_manager.set_state("127.0.0.1:9002", PeerState::KnownOnly);

        let state = NodeApiState::new(chain.clone(), mempool)
            .with_peer_manager(peer_manager)
            .with_miner_manager(miner_manager);

        let snapshot = state.status_snapshot().await;

        assert_eq!(snapshot.version, crate::config::constants::NODE_VERSION);
        assert_eq!(snapshot.canonical_tip_height, 1);
        assert_eq!(snapshot.canonical_tip_hash, expected_tip_hash);
        assert_eq!(snapshot.cached_state_root_height, Some(1));
        assert_eq!(snapshot.cached_state_root, Some(expected_state_root));
        assert_eq!(snapshot.mempool_size, 2);
        assert_eq!(snapshot.peer_count, 1);
        assert_eq!(snapshot.mining.available, true);
        assert_eq!(snapshot.mining.active, false);
        assert_eq!(snapshot.mining.blocks_found, 0);
    }

    #[tokio::test]
    async fn status_snapshot_does_not_mutate_chain_or_mempool() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        assert!(mempool.insert(placeholder_tx(0)));

        {
            let mut chain_guard = chain.lock().await;
            chain_guard.balances.insert("aa".repeat(32), 100);
            chain_guard.nonces.insert("aa".repeat(32), 7);
        }

        let state = NodeApiState::new(chain.clone(), mempool.clone());
        let before = {
            let chain_guard = chain.lock().await;
            (
                chain_guard.blocks.len(),
                chain_guard.balances.clone(),
                chain_guard.nonces.clone(),
                chain_guard.cached_state_root.clone(),
                mempool.list_ids(),
            )
        };

        let first = state.status_snapshot().await;
        let second = state.status_snapshot().await;

        let after = {
            let chain_guard = chain.lock().await;
            (
                chain_guard.blocks.len(),
                chain_guard.balances.clone(),
                chain_guard.nonces.clone(),
                chain_guard.cached_state_root.clone(),
                mempool.list_ids(),
            )
        };

        assert_eq!(first, second);
        assert_eq!(after, before);
    }

    #[test]
    fn status_snapshot_json_schema_is_deterministic() {
        let snapshot = NodeStatusSnapshot {
            version: crate::config::constants::NODE_VERSION,
            canonical_tip_height: 0,
            canonical_tip_hash: "00".repeat(32),
            cached_state_root_height: None,
            cached_state_root: None,
            mempool_size: 0,
            peer_count: 0,
            mining: MiningStatusSnapshot::unavailable(),
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            json,
            format!(
                "{{\"version\":\"{}\",\"canonical_tip_height\":0,\"canonical_tip_hash\":\"{}\",\"cached_state_root_height\":null,\"cached_state_root\":null,\"mempool_size\":0,\"peer_count\":0,\"mining\":{{\"available\":false,\"active\":false,\"blocks_found\":0}}}}",
                crate::config::constants::NODE_VERSION,
                "00".repeat(32),
            )
        );
    }
}



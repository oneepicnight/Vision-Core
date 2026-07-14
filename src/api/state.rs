use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::api::mining::MiningInfoResponse;
use crate::api::transactions::{submit_transaction as submit_transaction_service, TransactionSubmissionResult};
use crate::chain::{snapshots::save_snapshot, state_root::compute_state_root, ChainState};
use crate::mempool::Mempool;
use crate::types::transaction::canonical_tx_id;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AccountBalanceSnapshot {
    pub address: String,
    pub exists: bool,
    pub balance: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AccountNonceSnapshot {
    pub address: String,
    pub exists: bool,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TransactionLookupSnapshot {
    pub tx_id: String,
    pub found: bool,
    pub block_hash: Option<String>,
    pub block_height: Option<u64>,
    pub tx_index: Option<usize>,
    pub tx: Option<Tx>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AlphaAirdropSnapshot {
    pub status: &'static str,
    pub scope: &'static str,
    pub address: String,
    pub amount: u128,
    pub balance: u128,
    pub canonical_tip_height: u64,
    pub cached_state_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AlphaAirdropError {
    Disabled,
    InvalidAddress,
    ZeroAmount,
    StateRootComputationFailed,
    SnapshotPersistenceFailed,
}
#[derive(Clone)]
pub(crate) struct NodeApiState {
    chain: Arc<Mutex<ChainState>>,
    mempool: Arc<Mempool>,
    peer_manager: Option<Arc<PeerManager>>,
    miner_manager: Option<Arc<MinerManager>>,
    alpha_airdrop_enabled: bool,
}

impl NodeApiState {
    pub(crate) fn new(chain: Arc<Mutex<ChainState>>, mempool: Arc<Mempool>) -> Self {
        Self {
            chain,
            mempool,
            peer_manager: None,
            miner_manager: None,
            alpha_airdrop_enabled: false,
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

    pub(crate) fn with_alpha_airdrop_enabled(mut self, enabled: bool) -> Self {
        self.alpha_airdrop_enabled = enabled;
        self
    }

    pub(crate) fn has_miner_manager(&self) -> bool {
        self.miner_manager.is_some()
    }

    pub(crate) fn alpha_airdrop_enabled(&self) -> bool {
        self.alpha_airdrop_enabled
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
    pub(crate) async fn mining_info_snapshot(&self) -> MiningInfoResponse {
        let (height, difficulty) = {
            let chain = self.chain.lock().await;
            (chain.current_height(), chain.difficulty)
        };

        MiningInfoResponse {
            enabled: self.has_miner_manager(),
            height,
            difficulty,
            epoch: crate::pow::visionx::VISIONX_PARAMS.epoch(height),
            hash_rate_estimate: None,
        }
    }

    pub(crate) async fn balance_snapshot(&self, address: &str) -> AccountBalanceSnapshot {
        let chain = self.chain.lock().await;
        AccountBalanceSnapshot {
            address: address.to_string(),
            exists: chain.balances.contains_key(address),
            balance: chain.balance_of(address),
        }
    }

    pub(crate) async fn nonce_snapshot(&self, address: &str) -> AccountNonceSnapshot {
        let chain = self.chain.lock().await;
        AccountNonceSnapshot {
            address: address.to_string(),
            exists: chain.nonces.contains_key(address),
            nonce: chain.nonce_of(address),
        }
    }

    pub(crate) async fn transaction_snapshot(&self, tx_id: &str) -> TransactionLookupSnapshot {
        {
            let chain = self.chain.lock().await;
            for (block_height, block) in chain.blocks.iter().enumerate() {
                for (tx_index, tx) in block.txs.iter().enumerate() {
                    if canonical_tx_id(tx) == tx_id {
                        return TransactionLookupSnapshot {
                            tx_id: tx_id.to_string(),
                            found: true,
                            block_hash: Some(block.hash().to_string()),
                            block_height: Some(block_height as u64),
                            tx_index: Some(tx_index),
                            tx: Some(tx.clone()),
                        };
                    }
                }
            }
        }

        if let Some(tx) = self.mempool.get(tx_id) {
            return TransactionLookupSnapshot {
                tx_id: tx_id.to_string(),
                found: true,
                block_hash: None,
                block_height: None,
                tx_index: None,
                tx: Some(tx),
            };
        }

        TransactionLookupSnapshot {
            tx_id: tx_id.to_string(),
            found: false,
            block_hash: None,
            block_height: None,
            tx_index: None,
            tx: None,
        }
    }

    pub(crate) async fn alpha_airdrop(
        &self,
        address: &str,
        amount: u128,
    ) -> Result<AlphaAirdropSnapshot, AlphaAirdropError> {
        if !self.alpha_airdrop_enabled {
            return Err(AlphaAirdropError::Disabled);
        }
        if address.len() != 64 || !address.as_bytes().iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(AlphaAirdropError::InvalidAddress);
        }
        if amount == 0 {
            return Err(AlphaAirdropError::ZeroAmount);
        }

        let mut chain = self.chain.lock().await;
        let previous_balance = chain.balances.get(address).copied();
        let previous_cached_state_root = chain.cached_state_root.clone();

        chain.credit_balance(address, amount);

        let result = (|| -> Result<AlphaAirdropSnapshot, AlphaAirdropError> {
            let height = chain.current_height();
            let state_root = compute_state_root(&chain.balances, &chain.nonces)
                .map_err(|_| AlphaAirdropError::StateRootComputationFailed)?;
            chain.cached_state_root = Some((height, state_root.clone()));
            save_snapshot(&chain, height).map_err(|_| AlphaAirdropError::SnapshotPersistenceFailed)?;

            Ok(AlphaAirdropSnapshot {
                status: "accepted",
                scope: "alpha_dev_only",
                address: address.to_string(),
                amount,
                balance: chain.balance_of(address),
                canonical_tip_height: height,
                cached_state_root: state_root,
            })
        })();

        if result.is_err() {
            match previous_balance {
                Some(balance) => {
                    chain.balances.insert(address.to_string(), balance);
                }
                None => {
                    chain.balances.remove(address);
                }
            }
            chain.cached_state_root = previous_cached_state_root;
        }

        result
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

    #[tokio::test]
    async fn transaction_snapshot_returns_pending_mempool_tx() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let tx = placeholder_tx(9);
        let tx_id = canonical_tx_id(&tx);
        assert!(mempool.insert(tx.clone()));

        let state = NodeApiState::new(chain, mempool);
        let snapshot = state.transaction_snapshot(&tx_id).await;

        assert!(snapshot.found);
        assert_eq!(snapshot.tx_id, tx_id);
        assert_eq!(snapshot.block_hash, None);
        assert_eq!(snapshot.block_height, None);
        assert_eq!(snapshot.tx_index, None);
        assert_eq!(snapshot.tx, Some(tx));
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

















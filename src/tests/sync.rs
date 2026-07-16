#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use crate::chain::state::ChainState;
    use crate::p2p::connection::P2PConnectionManager;
    use crate::p2p::peer_manager::PeerManager;
    use crate::p2p::sync::{watchdog_step, SyncGuard};

    #[tokio::test]
    async fn watchdog_noop_when_synced() {
        let pm = Arc::new(PeerManager::new());
        let chain = Arc::new(Mutex::new({
            let db = sled::Config::new().temporary(true).open().unwrap();
            ChainState::empty(db)
        }));
        let conn_mgr = P2PConnectionManager::new(
            "127.0.0.1:19999".parse().unwrap(),
            chain.clone(),
            pm.clone(),
        );
        let mut guard = SyncGuard::new();
        watchdog_step(&conn_mgr, &chain, pm.as_ref(), &mut guard, None, None)
            .await
            .expect("watchdog should not error");
    }
}

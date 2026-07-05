#[cfg(test)]
mod tests {
    use crate::p2p::sync::{watchdog_step, SyncGuard};
    use crate::p2p::peer_manager::PeerManager;

    #[tokio::test]
    async fn watchdog_noop_when_synced() {
        let pm = PeerManager::new();
        let mut guard = SyncGuard::new();
        // No peers → best_remote_height = 0 → watchdog is a no-op.
        watchdog_step(&pm, 0, &mut guard).await.expect("watchdog should not error");
    }
}

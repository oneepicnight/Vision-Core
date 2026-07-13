use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use crate::config::constants::{PEER_HEIGHT_STALE_SECS, HEIGHT_POLL_RESPONSE_WINDOW_SECS};

/// Connection state of a peer.
#[derive(Debug, Clone, PartialEq)]
pub enum PeerState {
    /// TCP connection established and handshake completed.
    Connected,
    /// Connecting or handshake in progress.
    Connecting,
    /// Not currently connected.
    Disconnected,
    /// Address known but never connected.
    KnownOnly,
}

/// Per-peer metadata tracked by the PeerManager.
#[derive(Debug, Clone)]
pub struct Peer {
    /// Peer's canonical address ("host:port").
    pub addr: String,

    /// Current connection state.
    pub state: PeerState,

    /// Whether this is an outbound connection (we initiated it).
    pub is_outbound: bool,

    /// Last reported chain height from this peer.
    pub height: u64,

    /// Wall-clock instant when `height` was last updated.
    pub last_height_updated_at: Option<Instant>,

    /// Wall-clock instant of the last height poll we sent.
    pub last_height_poll_sent_at: Option<Instant>,

    /// Wall-clock instant of the last height response received.
    pub last_height_response_at: Option<Instant>,

    /// Wall-clock instant of the last message (any kind) received.
    pub last_activity: Option<Instant>,
}

impl Peer {
    pub fn new(addr: String, is_outbound: bool) -> Self {
        Self {
            addr,
            state: PeerState::KnownOnly,
            is_outbound,
            height: 0,
            last_height_updated_at: None,
            last_height_poll_sent_at: None,
            last_height_response_at: None,
            last_activity: None,
        }
    }

    /// True if the height record is fresh (updated within PEER_HEIGHT_STALE_SECS).
    pub fn is_height_fresh(&self) -> bool {
        self.last_height_updated_at
            .map(|t| t.elapsed().as_secs() < PEER_HEIGHT_STALE_SECS)
            .unwrap_or(false)
    }

    /// True if a pending height poll has gone unanswered too long.
    pub fn is_height_poll_stalled(&self) -> bool {
        match (self.last_height_poll_sent_at, self.last_height_response_at) {
            (Some(sent), Some(recv)) => {
                sent > recv && sent.elapsed().as_secs() >= HEIGHT_POLL_RESPONSE_WINDOW_SECS
            }
            (Some(sent), None) => {
                sent.elapsed().as_secs() >= HEIGHT_POLL_RESPONSE_WINDOW_SECS
            }
            _ => false,
        }
    }
}

/// Thread-safe peer registry.
///
/// PeerManager is the single authoritative source of peer heights.
/// Transport layers must not cache heights independently.
pub struct PeerManager {
    peers: Arc<RwLock<HashMap<String, Peer>>>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or update a peer address.
    pub fn upsert(&self, addr: &str, is_outbound: bool) {
        let mut peers = self.peers.write().unwrap();
        peers.entry(addr.to_string())
            .or_insert_with(|| Peer::new(addr.to_string(), is_outbound));
    }

    /// Update the connection state of a peer. Clears height on disconnect.
    pub fn set_state(&self, addr: &str, state: PeerState) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(addr) {
            let disconnecting = matches!(
                state,
                PeerState::Disconnected | PeerState::KnownOnly
            );
            peer.state = state;
            if disconnecting {
                // Clear stale height data on disconnect (Bug T1-1 from vision-node fix log).
                peer.height = 0;
                peer.last_height_updated_at = None;
                peer.last_height_response_at = None;
                peer.last_height_poll_sent_at = None;
            }
        }
    }

    /// Record a height received from a peer.
    ///
    /// Heights are monotonic while a peer remains connected: lower reports are
    /// treated as stale and only refresh activity timestamps.
    pub fn note_peer_height(&self, addr: &str, height: u64, _in_bulk_sync: bool) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(addr) {
            let now = Instant::now();
            if height < peer.height {
                peer.last_activity = Some(now);
                peer.last_height_response_at = Some(now);
                return;
            }
            peer.height = height;
            peer.last_height_updated_at = Some(now);
            peer.last_height_response_at = Some(now);
            peer.last_activity = Some(now);
        }
    }

    pub fn record_height_poll_sent(&self, addr: &str) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(addr) {
            peer.last_height_poll_sent_at = Some(Instant::now());
        }
    }

    /// Best known consensus height: max of connected+fresh peers (not stalled).
    pub fn best_remote_height(&self) -> u64 {
        let peers = self.peers.read().unwrap();
        peers.values()
            .filter(|p| {
                p.state == PeerState::Connected
                    && p.is_height_fresh()
                    && !p.is_height_poll_stalled()
            })
            .map(|p| p.height)
            .max()
            .unwrap_or(0)
    }

    /// Number of connected peers.
    pub fn connected_count(&self) -> usize {
        self.peers.read().unwrap()
            .values()
            .filter(|p| p.state == PeerState::Connected)
            .count()
    }

    /// Number of outbound connected peers.
    pub fn outbound_count(&self) -> usize {
        self.peers.read().unwrap()
            .values()
            .filter(|p| p.state == PeerState::Connected && p.is_outbound)
            .count()
    }

    /// Return a snapshot of all peers for display/API.
    pub fn snapshot(&self) -> Vec<Peer> {
        self.peers.read().unwrap().values().cloned().collect()
    }

    /// Return the address of the best peer to sync from.
    ///
    /// Criteria (all must hold):
    /// - State == `Connected`
    /// - Height is fresh (last update within `PEER_HEIGHT_STALE_SECS`)
    /// - Height poll is not stalled
    /// - Reported height > `local_height`
    ///
    /// Tiebreak: highest height wins. If heights are equal, the
    /// lexicographically smallest address wins (deterministic, testable).
    pub fn best_sync_target(&self, local_height: u64) -> Option<String> {
        let peers = self.peers.read().unwrap();
        peers.values()
            .filter(|p| {
                p.state == PeerState::Connected
                    && p.is_height_fresh()
                    && !p.is_height_poll_stalled()
                    && p.height > local_height
            })
            .max_by(|a, b| {
                a.height.cmp(&b.height)
                    // Lower address string wins on tie - deterministic.
                    .then_with(|| b.addr.cmp(&a.addr))
            })
            .map(|p| p.addr.clone())
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new()
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a PeerManager pre-loaded with connected peers at given heights.
    fn pm_with(peers: &[(&str, u64)]) -> PeerManager {
        let pm = PeerManager::new();
        for &(addr, height) in peers {
            pm.upsert(addr, true);
            pm.set_state(addr, PeerState::Connected);
            pm.note_peer_height(addr, height, false);
        }
        pm
    }

    // -- best_remote_height --

    #[test]
    fn best_remote_height_zero_when_no_peers() {
        let pm = PeerManager::new();
        assert_eq!(pm.best_remote_height(), 0);
    }

    #[test]
    fn best_remote_height_returns_max_connected_fresh() {
        let pm = pm_with(&[("a:9000", 50), ("b:9000", 100), ("c:9000", 75)]);
        assert_eq!(pm.best_remote_height(), 100);
    }

    #[test]
    fn best_remote_height_ignores_disconnected_peers() {
        let pm = pm_with(&[("a:9000", 200)]);
        pm.set_state("a:9000", PeerState::Disconnected);
        assert_eq!(pm.best_remote_height(), 0);
    }

    // -- best_sync_target --

    #[test]
    fn best_sync_target_none_when_no_peers() {
        let pm = PeerManager::new();
        assert!(pm.best_sync_target(0).is_none());
    }

    #[test]
    fn best_sync_target_none_when_all_at_or_below_local() {
        let pm = pm_with(&[("a:9000", 10), ("b:9000", 5)]);
        assert!(pm.best_sync_target(10).is_none());
    }

    #[test]
    fn best_sync_target_returns_highest_height_peer() {
        let pm = pm_with(&[("a:9000", 50), ("b:9000", 200), ("c:9000", 100)]);
        assert_eq!(pm.best_sync_target(0).unwrap(), "b:9000");
    }

    #[test]
    fn best_sync_target_tiebreak_is_deterministic_by_addr() {
        // Two peers at the same height - the lexicographically smaller addr wins.
        let pm = pm_with(&[("zzz:9000", 100), ("aaa:9000", 100)]);
        assert_eq!(pm.best_sync_target(0).unwrap(), "aaa:9000");
    }

    #[test]
    fn best_sync_target_ignores_disconnected_peer() {
        let pm = pm_with(&[("a:9000", 100), ("b:9000", 200)]);
        pm.set_state("b:9000", PeerState::Disconnected);
        // b was the best but is now disconnected; fall back to a.
        assert_eq!(pm.best_sync_target(0).unwrap(), "a:9000");
    }

    #[test]
    fn best_sync_target_excludes_peers_below_local() {
        let pm = pm_with(&[("a:9000", 50), ("b:9000", 99)]);
        // With local height = 100, neither peer qualifies.
        assert!(pm.best_sync_target(100).is_none());
    }

    // -- note_peer_height / in_bulk_sync guard --

    #[test]
    fn note_peer_height_updates_height() {
        let pm = PeerManager::new();
        pm.upsert("a:9000", false);
        pm.set_state("a:9000", PeerState::Connected);
        pm.note_peer_height("a:9000", 42, false);
        assert_eq!(pm.best_remote_height(), 42);
    }

    #[test]
    fn note_peer_height_in_bulk_sync_does_not_decrease_height() {
        let pm = PeerManager::new();
        pm.upsert("a:9000", false);
        pm.set_state("a:9000", PeerState::Connected);
        pm.note_peer_height("a:9000", 100, false);
        // Bulk sync delivers a lower height - must not overwrite.
        pm.note_peer_height("a:9000", 50, true);
        assert_eq!(pm.best_remote_height(), 100);
    }

    #[test]
    fn note_peer_height_does_not_regress_on_lower_connected_update() {
        let pm = PeerManager::new();
        pm.upsert("a:9000", false);
        pm.set_state("a:9000", PeerState::Connected);
        pm.note_peer_height("a:9000", 84, false);
        pm.note_peer_height("a:9000", 64, false);
        assert_eq!(pm.best_remote_height(), 84);
        pm.note_peer_height("a:9000", 87, false);
        assert_eq!(pm.best_remote_height(), 87);
    }

    #[test]
    fn reconnect_clears_height_and_refreshes_after_disconnect() {
        let pm = PeerManager::new();
        pm.upsert("a:9000", false);
        pm.set_state("a:9000", PeerState::Connected);
        pm.note_peer_height("a:9000", 84, false);
        pm.set_state("a:9000", PeerState::Disconnected);
        assert_eq!(pm.best_remote_height(), 0);
        pm.set_state("a:9000", PeerState::Connected);
        pm.note_peer_height("a:9000", 64, false);
        assert_eq!(pm.best_remote_height(), 64);
    }

    // -- set_state clears height on disconnect --

    #[test]
    fn disconnect_clears_height() {
        let pm = pm_with(&[("a:9000", 100)]);
        pm.set_state("a:9000", PeerState::Disconnected);
        // Height should be zeroed so it doesn't pollute best_remote_height.
        assert_eq!(pm.best_remote_height(), 0);
    }

    // -- connected_count / outbound_count --

    #[test]
    fn connected_count_and_outbound_count() {
        let pm = PeerManager::new();
        pm.upsert("in:9000", false);   // inbound
        pm.upsert("out:9000", true);   // outbound
        pm.set_state("in:9000",  PeerState::Connected);
        pm.set_state("out:9000", PeerState::Connected);
        assert_eq!(pm.connected_count(), 2);
        assert_eq!(pm.outbound_count(),  1);
    }
}

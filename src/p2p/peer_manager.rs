use crate::config::constants::{HEIGHT_POLL_RESPONSE_WINDOW_SECS, PEER_HEIGHT_STALE_SECS};
use crate::p2p::protocol::{ChainSummary, HandshakeMessage};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::Instant;

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
    /// Peers durable dial/identity address ("host:port").
    pub addr: String,

    /// Last observed TCP socket address for an active session.
    pub observed_addr: Option<String>,

    /// Validated advertised listening address, if supplied.
    pub advertised_addr: Option<String>,

    /// Current connection state.
    pub state: PeerState,

    /// Whether this is an outbound connection (we initiated it).
    pub is_outbound: bool,

    /// Last reported canonical chain height from this peer.
    pub height: u64,

    /// Last reported canonical tip hash from this peer.
    pub tip_hash: Option<String>,

    /// Last reported cumulative work for the peer's canonical tip.
    /// This is a discovery hint only; fork choice is still locally validated.
    pub cumulative_work: u128,

    /// Wall-clock instant when chain summary data was last updated.
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
            observed_addr: None,
            advertised_addr: None,
            is_outbound,
            height: 0,
            tip_hash: None,
            cumulative_work: 0,
            last_height_updated_at: None,
            last_height_poll_sent_at: None,
            last_height_response_at: None,
            last_activity: None,
        }
    }

    /// True if the chain summary record is fresh.
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
            (Some(sent), None) => sent.elapsed().as_secs() >= HEIGHT_POLL_RESPONSE_WINDOW_SECS,
            _ => false,
        }
    }

    pub fn summary(&self) -> ChainSummary {
        ChainSummary::new(self.height, self.tip_hash.clone(), self.cumulative_work)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerCounts {
    pub durable_peers: usize,
    pub active_inbound_sessions: usize,
    pub active_outbound_sessions: usize,
    pub transient_connections: usize,
    pub dialable_peers: usize,
}

fn validated_advertised_addr(
    host: Option<&str>,
    port: Option<u16>,
    allow_private: bool,
) -> Result<Option<String>, String> {
    let (Some(host), Some(port)) = (host, port) else {
        return Ok(None);
    };
    let host = host.trim();
    if host.is_empty() || port == 0 {
        return Err("malformed advertised address".to_string());
    }
    if host.contains(':') && host.parse::<IpAddr>().is_err() {
        return Err("malformed advertised host".to_string());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !allow_private && is_private_like_ip(ip) {
            return Err("private advertised address not allowed".to_string());
        }
    } else if !host
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-'))
    {
        return Err("malformed advertised host".to_string());
    }
    Ok(Some(format!("{}:{}", host.to_ascii_lowercase(), port)))
}

fn is_private_like_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Thread-safe peer registry.
///
/// PeerManager is the single authoritative source of peer chain summaries.
/// Transport layers must not cache heights or work independently.
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
        peers
            .entry(addr.to_string())
            .and_modify(|peer| {
                peer.is_outbound |= is_outbound;
            })
            .or_insert_with(|| Peer::new(addr.to_string(), is_outbound));
    }

    /// Update the connection state of a peer. Clears summary data on disconnect.
    pub fn set_state(&self, addr: &str, state: PeerState) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(addr) {
            let disconnecting = matches!(state, PeerState::Disconnected | PeerState::KnownOnly);
            peer.state = state;
            if disconnecting {
                peer.height = 0;
                peer.tip_hash = None;
                peer.cumulative_work = 0;
                peer.last_height_updated_at = None;
                peer.last_height_response_at = None;
                peer.last_height_poll_sent_at = None;
            }
        }
    }

    /// Record a height received from a handshake.
    ///
    /// Handshake height is intentionally height-only. It must not overwrite a
    /// fresher cumulative-work summary learned from `GetHeight`.
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

    /// Record a full chain summary received from a peer.
    ///
    /// Lower-height summaries are accepted when they advertise strictly more
    /// cumulative work; that is the condition needed to discover shorter but
    /// higher-work forks. Lower-work stale summaries only refresh activity.
    pub fn note_peer_summary(&self, addr: &str, summary: ChainSummary, _in_bulk_sync: bool) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(addr) {
            let now = Instant::now();
            let should_update = summary.cumulative_work > peer.cumulative_work
                || summary.height >= peer.height
                || peer.tip_hash.is_none();

            if should_update {
                tracing::debug!(
                    "[P2P] peer summary update addr={} old_height={} old_work={} new_height={} new_work={} new_tip={:?}",
                    addr,
                    peer.height,
                    peer.cumulative_work,
                    summary.height,
                    summary.cumulative_work,
                    summary.tip_hash
                );
                peer.height = summary.height;
                peer.tip_hash = summary.tip_hash;
                peer.cumulative_work = summary.cumulative_work;
                peer.last_height_updated_at = Some(now);
            }
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
        peers
            .values()
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
        self.peers
            .read()
            .unwrap()
            .values()
            .filter(|p| p.state == PeerState::Connected)
            .count()
    }

    /// Number of outbound connected peers.
    pub fn outbound_count(&self) -> usize {
        self.peers
            .read()
            .unwrap()
            .values()
            .filter(|p| p.state == PeerState::Connected && p.is_outbound)
            .count()
    }

    /// Return a snapshot of all peers for display/API.
    pub fn snapshot(&self) -> Vec<Peer> {
        self.peers.read().unwrap().values().cloned().collect()
    }

    /// Return the address of the best height-based peer to sync from.
    pub fn best_sync_target(&self, local_height: u64) -> Option<String> {
        let peers = self.peers.read().unwrap();
        peers
            .values()
            .filter(|p| {
                p.state == PeerState::Connected
                    && p.is_height_fresh()
                    && !p.is_height_poll_stalled()
                    && p.height > local_height
            })
            .max_by(|a, b| a.height.cmp(&b.height).then_with(|| b.addr.cmp(&a.addr)))
            .map(|p| p.addr.clone())
    }

    /// Return the best peer whose advertised canonical tip has strictly more
    /// work than the local tip. This only starts discovery; received blocks are
    /// still validated locally before any reorg can occur.
    pub fn best_work_sync_target(&self, local_tip_hash: &str, local_work: u128) -> Option<String> {
        let peers = self.peers.read().unwrap();
        peers
            .values()
            .filter(|p| {
                p.state == PeerState::Connected
                    && p.is_height_fresh()
                    && !p.is_height_poll_stalled()
                    && p.cumulative_work > local_work
                    && p.tip_hash.as_deref() != Some(local_tip_hash)
                    && p.tip_hash.is_some()
            })
            .max_by(|a, b| {
                a.cumulative_work
                    .cmp(&b.cumulative_work)
                    .then_with(|| a.height.cmp(&b.height))
                    .then_with(|| b.addr.cmp(&a.addr))
            })
            .map(|p| p.addr.clone())
    }



    pub fn peer_counts(&self) -> PeerCounts {
        let peers = self.peers.read().unwrap();
        let durable_peers = peers.len();
        let active_inbound_sessions = peers
            .values()
            .filter(|p| p.state == PeerState::Connected && !p.is_outbound)
            .count();
        let active_outbound_sessions = peers
            .values()
            .filter(|p| p.state == PeerState::Connected && p.is_outbound)
            .count();
        let transient_connections = peers
            .values()
            .filter(|p| p.state == PeerState::Connected && p.advertised_addr.is_none() && !p.is_outbound)
            .count();
        let dialable_peers = peers
            .values()
            .filter(|p| p.is_outbound || p.advertised_addr.is_some())
            .count();
        PeerCounts {
            durable_peers,
            active_inbound_sessions,
            active_outbound_sessions,
            transient_connections,
            dialable_peers,
        }
    }

    pub fn resolve_inbound_peer_key(
        &self,
        observed_addr: &str,
        handshake: &HandshakeMessage,
        allow_private: bool,
    ) -> Result<String, String> {
        let durable_addr = match validated_advertised_addr(
            handshake.advertised_ip.as_deref(),
            handshake.advertised_port,
            allow_private,
        )? {
            Some(addr) => addr,
            None => observed_addr.to_string(),
        };

        let mut peers = self.peers.write().unwrap();
        let peer = peers
            .entry(durable_addr.clone())
            .or_insert_with(|| Peer::new(durable_addr.clone(), false));
        peer.observed_addr = Some(observed_addr.to_string());
        if durable_addr != observed_addr {
            peer.advertised_addr = Some(durable_addr.clone());
        }
        Ok(durable_addr)
    }

    pub fn note_observed_addr(&self, peer_addr: &str, observed_addr: &str) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(peer_addr) {
            peer.observed_addr = Some(observed_addr.to_string());
        }
    }

    pub fn peer_summary(&self, addr: &str) -> Option<ChainSummary> {
        self.peers.read().unwrap().get(addr).map(Peer::summary)
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm_with(peers: &[(&str, u64)]) -> PeerManager {
        let pm = PeerManager::new();
        for &(addr, height) in peers {
            pm.upsert(addr, true);
            pm.set_state(addr, PeerState::Connected);
            pm.note_peer_height(addr, height, false);
        }
        pm
    }


    fn advertised_hs(host: &str, port: u16) -> HandshakeMessage {
        let mut hs = HandshakeMessage::new(0, 42);
        hs.advertised_ip = Some(host.to_string());
        hs.advertised_port = Some(port);
        hs
    }

    #[test]
    fn valid_inbound_advertised_identity_becomes_durable_peer() {
        let pm = PeerManager::new();
        let key = pm
            .resolve_inbound_peer_key("127.0.0.1:51000", &advertised_hs("127.0.0.1", 9001), true)
            .unwrap();
        assert_eq!(key, "127.0.0.1:9001");
        pm.set_state(&key, PeerState::Connected);
        let counts = pm.peer_counts();
        assert_eq!(counts.durable_peers, 1);
        assert_eq!(counts.active_inbound_sessions, 1);
        assert_eq!(counts.dialable_peers, 1);
        assert_eq!(counts.transient_connections, 0);
    }

    #[test]
    fn inbound_without_identity_is_transient_non_dialable() {
        let pm = PeerManager::new();
        let hs = HandshakeMessage::new(0, 42);
        let key = pm.resolve_inbound_peer_key("127.0.0.1:51000", &hs, true).unwrap();
        assert_eq!(key, "127.0.0.1:51000");
        pm.set_state(&key, PeerState::Connected);
        let counts = pm.peer_counts();
        assert_eq!(counts.durable_peers, 1);
        assert_eq!(counts.transient_connections, 1);
        assert_eq!(counts.dialable_peers, 0);
    }

    #[test]
    fn source_port_churn_deduplicates_by_advertised_identity() {
        let pm = PeerManager::new();
        let hs = advertised_hs("127.0.0.1", 9001);
        let first = pm.resolve_inbound_peer_key("127.0.0.1:51000", &hs, true).unwrap();
        let second = pm.resolve_inbound_peer_key("127.0.0.1:51001", &hs, true).unwrap();
        assert_eq!(first, second);
        assert_eq!(pm.peer_counts().durable_peers, 1);
    }

    #[test]
    fn public_mode_rejects_private_advertised_address() {
        let pm = PeerManager::new();
        let err = pm
            .resolve_inbound_peer_key("198.51.100.7:51000", &advertised_hs("127.0.0.1", 9001), false)
            .unwrap_err();
        assert!(err.contains("private advertised address"));
    }

    #[test]
    fn inbound_higher_work_peer_is_sync_target() {
        let pm = PeerManager::new();
        let key = pm
            .resolve_inbound_peer_key("127.0.0.1:51000", &advertised_hs("127.0.0.1", 9001), true)
            .unwrap();
        pm.set_state(&key, PeerState::Connected);
        pm.note_peer_summary(
            &key,
            ChainSummary::new(81, Some("remote".to_string()), 1757),
            false,
        );
        assert_eq!(pm.best_work_sync_target("local", 1754), Some(key));
    }

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
        let pm = pm_with(&[("zzz:9000", 100), ("aaa:9000", 100)]);
        assert_eq!(pm.best_sync_target(0).unwrap(), "aaa:9000");
    }

    #[test]
    fn best_sync_target_ignores_disconnected_peer() {
        let pm = pm_with(&[("a:9000", 100), ("b:9000", 200)]);
        pm.set_state("b:9000", PeerState::Disconnected);
        assert_eq!(pm.best_sync_target(0).unwrap(), "a:9000");
    }

    #[test]
    fn best_sync_target_excludes_peers_below_local() {
        let pm = pm_with(&[("a:9000", 50), ("b:9000", 99)]);
        assert!(pm.best_sync_target(100).is_none());
    }

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
        pm.note_peer_summary(
            "a:9000",
            ChainSummary::new(84, Some("84".to_string()), 84),
            false,
        );
        pm.set_state("a:9000", PeerState::Disconnected);
        assert_eq!(pm.best_remote_height(), 0);
        assert!(pm.peer_summary("a:9000").unwrap().tip_hash.is_none());
        pm.set_state("a:9000", PeerState::Connected);
        pm.note_peer_summary(
            "a:9000",
            ChainSummary::new(64, Some("64".to_string()), 64),
            false,
        );
        assert_eq!(pm.best_remote_height(), 64);
        assert_eq!(
            pm.peer_summary("a:9000").unwrap().tip_hash.as_deref(),
            Some("64")
        );
    }

    #[test]
    fn disconnect_clears_height() {
        let pm = pm_with(&[("a:9000", 100)]);
        pm.set_state("a:9000", PeerState::Disconnected);
        assert_eq!(pm.best_remote_height(), 0);
    }

    #[test]
    fn connected_count_and_outbound_count() {
        let pm = PeerManager::new();
        pm.upsert("in:9000", false);
        pm.upsert("out:9000", true);
        pm.set_state("in:9000", PeerState::Connected);
        pm.set_state("out:9000", PeerState::Connected);
        assert_eq!(pm.connected_count(), 2);
        assert_eq!(pm.outbound_count(), 1);
    }

    #[test]
    fn lower_height_higher_work_summary_is_recorded() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(83, Some("old".to_string()), 1754),
            false,
        );
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("new".to_string()), 1757),
            false,
        );
        let summary = pm.peer_summary("c:9000").unwrap();
        assert_eq!(summary.height, 81);
        assert_eq!(summary.cumulative_work, 1757);
        assert_eq!(summary.tip_hash.as_deref(), Some("new"));
    }

    #[test]
    fn stale_lower_work_summary_does_not_suppress_newer_work() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("new".to_string()), 1757),
            false,
        );
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(64, Some("old".to_string()), 1000),
            false,
        );
        let summary = pm.peer_summary("c:9000").unwrap();
        assert_eq!(summary.height, 81);
        assert_eq!(summary.cumulative_work, 1757);
        assert_eq!(summary.tip_hash.as_deref(), Some("new"));
    }

    #[test]
    fn work_sync_target_detects_shorter_higher_work_peer() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("remote".to_string()), 1757),
            false,
        );
        assert_eq!(pm.best_work_sync_target("local", 1754).unwrap(), "c:9000");
    }

    #[test]
    fn work_sync_target_ignores_equal_work_branch() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("remote".to_string()), 1754),
            false,
        );
        assert!(pm.best_work_sync_target("local", 1754).is_none());
    }

    #[test]
    fn work_sync_target_ignores_same_tip() {
        let pm = PeerManager::new();
        pm.upsert("c:9000", true);
        pm.set_state("c:9000", PeerState::Connected);
        pm.note_peer_summary(
            "c:9000",
            ChainSummary::new(81, Some("same".to_string()), 1757),
            false,
        );
        assert!(pm.best_work_sync_target("same", 1754).is_none());
    }
}

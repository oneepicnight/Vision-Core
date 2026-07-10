use crate::config::constants::*;

/// Runtime-configurable node settings, populated from environment variables
/// and optional config file at startup. All fields have sane defaults.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Directory where the sled database and snapshots are stored.
    pub data_dir: String,

    /// HTTP API listen address.
    pub http_addr: String,

    /// P2P listen address.
    pub p2p_addr: String,

    /// Canonical reward recipient used by the miner.
    pub miner_address: String,

    /// Whether this node should attempt to mine blocks.
    pub mining_enabled: bool,

    /// Number of mining worker threads (0 = use logical CPU count).
    pub mining_threads: usize,

    /// Whether the Alpha-only local airdrop endpoint is enabled.
    pub alpha_airdrop_enabled: bool,

    /// Seed peer addresses to connect to on startup.
    pub seed_peers: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            data_dir: std::env::var("VISION_DATA_DIR")
                .unwrap_or_else(|_| "./data".into()),
            http_addr: format!(
                "0.0.0.0:{}",
                std::env::var("VISION_HTTP_PORT")
                    .ok()
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(DEFAULT_HTTP_PORT)
            ),
            p2p_addr: format!(
                "0.0.0.0:{}",
                std::env::var("VISION_P2P_PORT")
                    .ok()
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(DEFAULT_P2P_PORT)
            ),
            miner_address: parse_miner_address(std::env::var("VISION_MINER_ADDRESS").ok()),
            mining_enabled: std::env::var("VISION_MINING")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
            mining_threads: std::env::var("VISION_MINING_THREADS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            alpha_airdrop_enabled: std::env::var("VISION_ALPHA_AIRDROP_ENABLED")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
            seed_peers: parse_seed_peers(std::env::var("VISION_SEED_PEERS").ok()),
        }
    }
}

impl Settings {
    /// Load settings from the environment. Extend here to also read a TOML
    /// config file if `VISION_CONFIG` is set.
    pub fn from_env() -> Self {
        Self::default()
    }
}

fn parse_seed_peers(raw: Option<String>) -> Vec<String> {
    match raw {
        Some(raw) => raw
            .split([',', ';', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        None => DEFAULT_SEED_PEERS.iter().map(|s| s.to_string()).collect(),
    }
}

fn parse_miner_address(raw: Option<String>) -> String {
    let zero_address = "0".repeat(64);
    let Some(raw) = raw else {
        return zero_address;
    };

    let is_valid = raw.len() == 64
        && raw
            .as_bytes()
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));

    if is_valid {
        raw
    } else {
        zero_address
    }
}

#[cfg(test)]
mod tests {
    use super::parse_seed_peers;

    #[test]
    fn parse_seed_peers_uses_defaults_when_unset() {
        let peers = parse_seed_peers(None);
        assert!(!peers.is_empty());
    }

    #[test]
    fn parse_seed_peers_allows_empty_override() {
        let peers = parse_seed_peers(Some(String::new()));
        assert!(peers.is_empty());
    }

    #[test]
    fn parse_seed_peers_splits_and_trims() {
        let peers = parse_seed_peers(Some(" 127.0.0.1:7072 , 10.0.0.1:8080;\n192.168.1.1:9000 ".to_string()));
        assert_eq!(
            peers,
            vec![
                "127.0.0.1:7072".to_string(),
                "10.0.0.1:8080".to_string(),
                "192.168.1.1:9000".to_string(),
            ]
        );
    }
}
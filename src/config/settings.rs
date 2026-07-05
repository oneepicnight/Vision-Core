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

    /// Whether this node should attempt to mine blocks.
    pub mining_enabled: bool,

    /// Number of mining worker threads (0 = use logical CPU count).
    pub mining_threads: usize,

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
            mining_enabled: std::env::var("VISION_MINING")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
            mining_threads: std::env::var("VISION_MINING_THREADS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            seed_peers: DEFAULT_SEED_PEERS.iter().map(|s| s.to_string()).collect(),
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

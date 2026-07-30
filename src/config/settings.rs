use crate::config::constants::*;

trait SettingsSource {
    fn read(&self, name: &'static str) -> Option<String>;
}

struct EnvironmentSettingsSource;

impl SettingsSource for EnvironmentSettingsSource {
    fn read(&self, name: &'static str) -> Option<String> {
        std::env::var(name).ok()
    }
}

#[derive(Debug)]
struct RawSettings {
    data_dir: Option<String>,
    http_port: Option<String>,
    p2p_port: Option<String>,
    p2p_advertised_host: Option<String>,
    p2p_advertised_port: Option<String>,
    allow_private_peer_addresses: Option<String>,
    miner_address: Option<String>,
    mining_enabled: Option<String>,
    mining_threads: Option<String>,
    alpha_airdrop_enabled: Option<String>,
    seed_peers: Option<String>,
}

impl RawSettings {
    fn from_source(source: &impl SettingsSource) -> Self {
        Self {
            data_dir: source.read("VISION_DATA_DIR"),
            http_port: source.read("VISION_HTTP_PORT"),
            p2p_port: source.read("VISION_P2P_PORT"),
            p2p_advertised_host: source.read("VISION_P2P_ADVERTISED_HOST"),
            p2p_advertised_port: source.read("VISION_P2P_ADVERTISED_PORT"),
            allow_private_peer_addresses: source.read("VISION_ALLOW_PRIVATE_PEERS"),
            miner_address: source.read("VISION_MINER_ADDRESS"),
            mining_enabled: source.read("VISION_MINING"),
            mining_threads: source.read("VISION_MINING_THREADS"),
            alpha_airdrop_enabled: source.read("VISION_ALPHA_AIRDROP_ENABLED"),
            seed_peers: source.read("VISION_SEED_PEERS"),
        }
    }
}

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

    /// Optional advertised P2P host/IP used as this node's durable peer identity.
    pub p2p_advertised_host: Option<String>,

    /// Optional advertised P2P port used as this node's durable peer identity.
    pub p2p_advertised_port: Option<u16>,

    /// Whether loopback/private/link-local advertised peer addresses are accepted.
    pub allow_private_peer_addresses: bool,

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
        Self::from_source(&EnvironmentSettingsSource)
    }
}

impl Settings {
    /// Load settings from the environment. Extend here to also read a TOML
    /// config file if `VISION_CONFIG` is set.
    pub fn from_env() -> Self {
        Self::default()
    }

    fn from_source(source: &impl SettingsSource) -> Self {
        Self::from_raw(RawSettings::from_source(source))
    }

    fn from_raw(raw: RawSettings) -> Self {
        Self {
            data_dir: raw.data_dir.unwrap_or_else(|| "./data".into()),
            http_addr: format!(
                "0.0.0.0:{}",
                raw.http_port
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(DEFAULT_HTTP_PORT)
            ),
            p2p_addr: format!(
                "0.0.0.0:{}",
                raw.p2p_port
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(DEFAULT_P2P_PORT)
            ),
            p2p_advertised_host: parse_optional_string(raw.p2p_advertised_host),
            p2p_advertised_port: raw
                .p2p_advertised_port
                .and_then(|p| p.parse::<u16>().ok())
                .filter(|port| *port != 0),
            allow_private_peer_addresses: raw
                .allow_private_peer_addresses
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(true),
            miner_address: parse_miner_address(raw.miner_address),
            mining_enabled: raw
                .mining_enabled
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
            mining_threads: raw.mining_threads.and_then(|v| v.parse().ok()).unwrap_or(0),
            alpha_airdrop_enabled: raw
                .alpha_airdrop_enabled
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
            seed_peers: parse_seed_peers(raw.seed_peers),
        }
    }
}

fn parse_optional_string(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
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
    use std::collections::BTreeMap;
    use std::process::{Command, Output};

    use super::{
        parse_miner_address, parse_optional_string, parse_seed_peers, Settings, SettingsSource,
    };
    use crate::config::constants::{DEFAULT_HTTP_PORT, DEFAULT_P2P_PORT, DEFAULT_SEED_PEERS};

    const SETTINGS_ENV_VARS: &[&str] = &[
        "VISION_DATA_DIR",
        "VISION_HTTP_PORT",
        "VISION_P2P_PORT",
        "VISION_P2P_ADVERTISED_HOST",
        "VISION_P2P_ADVERTISED_PORT",
        "VISION_ALLOW_PRIVATE_PEERS",
        "VISION_MINER_ADDRESS",
        "VISION_MINING",
        "VISION_MINING_THREADS",
        "VISION_ALPHA_AIRDROP_ENABLED",
        "VISION_SEED_PEERS",
    ];
    const SETTINGS_PROBE_SCENARIO: &str = "VISION_TEST_SETTINGS_PROBE_SCENARIO";

    #[derive(Default)]
    struct TestSettingsSource {
        values: BTreeMap<&'static str, String>,
    }

    impl TestSettingsSource {
        fn with_values(values: &[(&'static str, &str)]) -> Self {
            Self {
                values: values
                    .iter()
                    .map(|(name, value)| (*name, (*value).to_string()))
                    .collect(),
            }
        }
    }

    impl SettingsSource for TestSettingsSource {
        fn read(&self, name: &'static str) -> Option<String> {
            self.values.get(name).cloned()
        }
    }

    fn run_settings_probe(scenario: &str, environment: &[(&str, &str)]) -> Output {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .arg("--exact")
            .arg("config::settings::tests::settings_subprocess_probe")
            .arg("--test-threads=1")
            .env(SETTINGS_PROBE_SCENARIO, scenario);

        for variable in SETTINGS_ENV_VARS {
            command.env_remove(variable);
        }
        for (variable, value) in environment {
            command.env(variable, value);
        }

        command.output().expect("settings probe should start")
    }

    fn assert_probe_succeeds(scenario: &str, environment: &[(&str, &str)]) {
        let output = run_settings_probe(scenario, environment);
        assert!(
            output.status.success(),
            "settings probe {scenario:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

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
        let peers = parse_seed_peers(Some(
            " 127.0.0.1:7072 , 10.0.0.1:8080;\n192.168.1.1:9000 ".to_string(),
        ));
        assert_eq!(
            peers,
            vec![
                "127.0.0.1:7072".to_string(),
                "10.0.0.1:8080".to_string(),
                "192.168.1.1:9000".to_string(),
            ]
        );
    }

    #[test]
    fn parse_optional_string_trims_and_rejects_empty_values() {
        assert_eq!(
            parse_optional_string(Some("  peer.example  ".to_string())),
            Some("peer.example".to_string())
        );
        assert_eq!(parse_optional_string(Some(" \t ".to_string())), None);
        assert_eq!(parse_optional_string(None), None);
    }

    #[test]
    fn parse_miner_address_accepts_only_lowercase_64_character_hex() {
        let valid = "0123456789abcdef".repeat(4);
        assert_eq!(parse_miner_address(Some(valid.clone())), valid);
        assert_eq!(parse_miner_address(Some("A".repeat(64))), "0".repeat(64));
        assert_eq!(parse_miner_address(Some("f".repeat(63))), "0".repeat(64));
        assert_eq!(parse_miner_address(None), "0".repeat(64));
    }

    #[test]
    fn typed_settings_seam_preserves_defaults() {
        let settings = Settings::from_source(&TestSettingsSource::default());

        assert_eq!(settings.data_dir, "./data");
        assert_eq!(settings.http_addr, format!("0.0.0.0:{DEFAULT_HTTP_PORT}"));
        assert_eq!(settings.p2p_addr, format!("0.0.0.0:{DEFAULT_P2P_PORT}"));
        assert_eq!(settings.p2p_advertised_host, None);
        assert_eq!(settings.p2p_advertised_port, None);
        assert!(settings.allow_private_peer_addresses);
        assert_eq!(settings.miner_address, "0".repeat(64));
        assert!(!settings.mining_enabled);
        assert_eq!(settings.mining_threads, 0);
        assert!(!settings.alpha_airdrop_enabled);
        assert_eq!(
            settings.seed_peers,
            DEFAULT_SEED_PEERS
                .iter()
                .map(|peer| peer.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn data_directory_input_is_currently_preserved_verbatim() {
        let cases = [
            ("", ""),
            ("   ", "   "),
            (" relative-data ", " relative-data "),
            ("relative-data", "relative-data"),
        ];

        for (raw, expected) in cases {
            let settings = Settings::from_source(&TestSettingsSource::with_values(&[(
                "VISION_DATA_DIR",
                raw,
            )]));
            assert_eq!(settings.data_dir, expected);
        }
    }

    #[test]
    fn typed_settings_seam_preserves_valid_and_invalid_parsing() {
        let settings = Settings::from_source(&TestSettingsSource::with_values(&[
            ("VISION_DATA_DIR", ""),
            ("VISION_HTTP_PORT", "17070"),
            ("VISION_P2P_PORT", "not-a-port"),
            ("VISION_P2P_ADVERTISED_HOST", " peer.example "),
            ("VISION_P2P_ADVERTISED_PORT", "0"),
            ("VISION_ALLOW_PRIVATE_PEERS", "yes"),
            (
                "VISION_MINER_ADDRESS",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
            ("VISION_MINING", "TrUe"),
            ("VISION_MINING_THREADS", "many"),
            ("VISION_ALPHA_AIRDROP_ENABLED", "1"),
            ("VISION_SEED_PEERS", "127.0.0.1:17072; 127.0.0.1:17073"),
        ]));

        assert_eq!(settings.data_dir, "");
        assert_eq!(settings.http_addr, "0.0.0.0:17070");
        assert_eq!(settings.p2p_addr, format!("0.0.0.0:{DEFAULT_P2P_PORT}"));
        assert_eq!(
            settings.p2p_advertised_host,
            Some("peer.example".to_string())
        );
        assert_eq!(settings.p2p_advertised_port, None);
        assert!(!settings.allow_private_peer_addresses);
        assert_eq!(
            settings.miner_address,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert!(settings.mining_enabled);
        assert_eq!(settings.mining_threads, 0);
        assert!(settings.alpha_airdrop_enabled);
        assert_eq!(
            settings.seed_peers,
            vec!["127.0.0.1:17072".to_string(), "127.0.0.1:17073".to_string()]
        );
    }

    #[test]
    fn settings_defaults_are_characterized_in_isolation() {
        assert_probe_succeeds("defaults", &[]);
    }

    #[test]
    fn settings_valid_environment_is_characterized_in_isolation() {
        assert_probe_succeeds(
            "valid",
            &[
                ("VISION_DATA_DIR", "custom-data"),
                ("VISION_HTTP_PORT", "17070"),
                ("VISION_P2P_PORT", "17072"),
                ("VISION_P2P_ADVERTISED_HOST", " peer.example "),
                ("VISION_P2P_ADVERTISED_PORT", "17073"),
                ("VISION_ALLOW_PRIVATE_PEERS", "TrUe"),
                (
                    "VISION_MINER_ADDRESS",
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                ),
                ("VISION_MINING", "1"),
                ("VISION_MINING_THREADS", "3"),
                ("VISION_ALPHA_AIRDROP_ENABLED", "TRUE"),
                ("VISION_SEED_PEERS", "127.0.0.1:17072; 127.0.0.1:17073"),
            ],
        );
    }

    #[test]
    fn settings_invalid_values_use_current_fallbacks_in_isolation() {
        assert_probe_succeeds(
            "invalid",
            &[
                ("VISION_DATA_DIR", ""),
                ("VISION_HTTP_PORT", "not-a-port"),
                ("VISION_P2P_PORT", "70000"),
                ("VISION_P2P_ADVERTISED_HOST", " \t "),
                ("VISION_P2P_ADVERTISED_PORT", "0"),
                ("VISION_ALLOW_PRIVATE_PEERS", "yes"),
                ("VISION_MINER_ADDRESS", "ABC"),
                ("VISION_MINING", "enabled"),
                ("VISION_MINING_THREADS", "many"),
                ("VISION_ALPHA_AIRDROP_ENABLED", "yes"),
                ("VISION_SEED_PEERS", ""),
            ],
        );
    }

    #[test]
    fn settings_subprocess_probe() {
        let Ok(scenario) = std::env::var(SETTINGS_PROBE_SCENARIO) else {
            return;
        };

        let settings = Settings::from_env();
        match scenario.as_str() {
            "defaults" => {
                assert_eq!(settings.data_dir, "./data");
                assert_eq!(settings.http_addr, format!("0.0.0.0:{DEFAULT_HTTP_PORT}"));
                assert_eq!(settings.p2p_addr, format!("0.0.0.0:{DEFAULT_P2P_PORT}"));
                assert_eq!(settings.p2p_advertised_host, None);
                assert_eq!(settings.p2p_advertised_port, None);
                assert!(settings.allow_private_peer_addresses);
                assert_eq!(settings.miner_address, "0".repeat(64));
                assert!(!settings.mining_enabled);
                assert_eq!(settings.mining_threads, 0);
                assert!(!settings.alpha_airdrop_enabled);
                assert_eq!(
                    settings.seed_peers,
                    DEFAULT_SEED_PEERS
                        .iter()
                        .map(|peer| peer.to_string())
                        .collect::<Vec<_>>()
                );
            }
            "valid" => {
                assert_eq!(settings.data_dir, "custom-data");
                assert_eq!(settings.http_addr, "0.0.0.0:17070");
                assert_eq!(settings.p2p_addr, "0.0.0.0:17072");
                assert_eq!(
                    settings.p2p_advertised_host,
                    Some("peer.example".to_string())
                );
                assert_eq!(settings.p2p_advertised_port, Some(17073));
                assert!(settings.allow_private_peer_addresses);
                assert_eq!(
                    settings.miner_address,
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                );
                assert!(settings.mining_enabled);
                assert_eq!(settings.mining_threads, 3);
                assert!(settings.alpha_airdrop_enabled);
                assert_eq!(
                    settings.seed_peers,
                    vec!["127.0.0.1:17072".to_string(), "127.0.0.1:17073".to_string()]
                );
            }
            "invalid" => {
                assert_eq!(settings.data_dir, "");
                assert_eq!(settings.http_addr, format!("0.0.0.0:{DEFAULT_HTTP_PORT}"));
                assert_eq!(settings.p2p_addr, format!("0.0.0.0:{DEFAULT_P2P_PORT}"));
                assert_eq!(settings.p2p_advertised_host, None);
                assert_eq!(settings.p2p_advertised_port, None);
                assert!(!settings.allow_private_peer_addresses);
                assert_eq!(settings.miner_address, "0".repeat(64));
                assert!(!settings.mining_enabled);
                assert_eq!(settings.mining_threads, 0);
                assert!(!settings.alpha_airdrop_enabled);
                assert!(settings.seed_peers.is_empty());
            }
            unexpected => panic!("unexpected settings probe scenario: {unexpected}"),
        }
    }
}

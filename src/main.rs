//! vision-core - minimal stable blockchain node
//!
//! main.rs is responsible only for startup wiring:
//! - parse settings from environment
//! - print startup banner
//! - initialise chain state and genesis
//! - start API and P2P services
//! - block on the Tokio runtime
//!
//! All protocol logic lives in the dedicated modules under `src/`.

mod api;
mod chain;
mod config;
mod genesis;
mod mempool;
mod miner;
mod node;
mod p2p;
mod pow;
mod tests;
mod types;

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use config::constants::*;
use config::settings::Settings;

fn main() {
    let rt = match node::runtime::build_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Fatal: {error}");
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(async_main()) {
        tracing::error!("Fatal: {}", e);
        std::process::exit(1);
    }
}

async fn async_main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut settings = Settings::from_env()?;
    node::services::resolve_auto_p2p_settings(&mut settings)?;

    print_banner(&settings);

    let chain_state = node::bootstrap::initialize_chain_state(&settings)?;

    let chain = Arc::new(Mutex::new(chain_state));
    let peer_manager = Arc::new(p2p::peer_manager::PeerManager::new());
    let mempool = Arc::new(mempool::Mempool::new());
    let miner_manager = settings
        .mining_enabled
        .then(|| Arc::new(miner::MinerManager::new(*pow::visionx::VISIONX_PARAMS)));
    let recovery_state = Arc::new(node::recovery::RecoveryState::new());

    let seed_addrs = node::bootstrap::seed_peers(&settings);
    for addr in &seed_addrs {
        peer_manager.upsert(addr, true);
    }
    tracing::info!("[NODE] {} seed peers loaded", seed_addrs.len());

    node::services::start_services(
        chain.clone(),
        peer_manager.clone(),
        mempool.clone(),
        miner_manager.clone(),
        recovery_state.clone(),
        &settings,
    )
    .await?;

    let mut api_state = api::state::NodeApiState::new(chain.clone(), mempool)
        .with_peer_manager(peer_manager.clone())
        .with_recovery_state(recovery_state.clone())
        .with_alpha_airdrop_enabled(settings.alpha_airdrop_enabled);
    if let Some(miner_manager) = miner_manager {
        api_state = api_state.with_miner_manager(miner_manager);
    }
    let app = api::routes::api_router(api_state);
    let http_addr: std::net::SocketAddr = settings.http_addr.parse()?;
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!("[API] Listening on http://{}", http_addr);
    tracing::info!("[NODE] All services started");

    axum::serve(http_listener, app).await?;

    Ok(())
}

fn print_banner(settings: &Settings) {
    println!(
        r#"
+--------------------------------------------------------------+
|                      vision-core {}                       |
|
|  Network  : {}
|  P2P      : {}
|  API      : {}
|  Mining   : {}
|  Data     : {}
+--------------------------------------------------------------+
"#,
        NODE_VERSION,
        NETWORK_ID,
        settings.p2p_addr,
        settings.http_addr,
        if settings.mining_enabled {
            "enabled"
        } else {
            "disabled"
        },
        settings.data_dir,
    );
    tracing::info!("vision-core {} starting", NODE_VERSION);
}

#[cfg(test)]
mod startup_tests {
    use super::async_main;
    use std::net::TcpListener as StdTcpListener;
    use std::process::{Command, Output};

    const STARTUP_PROBE_SCENARIO: &str = "VISION_TEST_STARTUP_PROBE_SCENARIO";
    const STARTUP_ENV_VARS: &[&str] = &[
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

    fn run_startup_probe(scenario: &str, environment: &[(&str, String)]) -> Output {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .arg("--exact")
            .arg("startup_tests::startup_subprocess_probe")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(STARTUP_PROBE_SCENARIO, scenario);

        for variable in STARTUP_ENV_VARS {
            command.env_remove(variable);
        }
        for (variable, value) in environment {
            command.env(variable, value);
        }

        command.output().expect("startup probe should start")
    }

    fn assert_probe_fails_without_started_log(scenario: &str, environment: &[(&str, String)]) {
        let output = run_startup_probe(scenario, environment);
        assert!(
            !output.status.success(),
            "startup probe {scenario:?} unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let output_text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output_text.contains("[NODE] All services started"),
            "startup probe {scenario:?} reported the node as started\noutput:\n{output_text}"
        );
        assert!(
            !output_text.contains("[API] Listening on http://"),
            "startup probe {scenario:?} reported API readiness before bind succeeded\noutput:\n{output_text}"
        );
    }

    #[test]
    fn startup_rejects_occupied_p2p_listener_before_started_log() {
        let data_dir = tempfile::tempdir().expect("temporary data directory");
        let occupied_listener = StdTcpListener::bind(("0.0.0.0", 0)).expect("occupied port");
        let occupied_port = occupied_listener.local_addr().unwrap().port();

        assert_probe_fails_without_started_log(
            "occupied_p2p_listener",
            &[
                ("VISION_DATA_DIR", data_dir.path().display().to_string()),
                ("VISION_P2P_PORT", occupied_port.to_string()),
                ("VISION_HTTP_PORT", "0".to_string()),
                ("VISION_SEED_PEERS", String::new()),
            ],
        );
    }

    #[test]
    fn startup_does_not_report_started_when_http_bind_fails() {
        let data_dir = tempfile::tempdir().expect("temporary data directory");
        let occupied_listener = StdTcpListener::bind(("0.0.0.0", 0)).expect("occupied port");
        let occupied_port = occupied_listener.local_addr().unwrap().port();

        assert_probe_fails_without_started_log(
            "occupied_http_listener",
            &[
                ("VISION_DATA_DIR", data_dir.path().display().to_string()),
                ("VISION_P2P_PORT", "0".to_string()),
                ("VISION_HTTP_PORT", occupied_port.to_string()),
                ("VISION_SEED_PEERS", String::new()),
            ],
        );
    }

    #[test]
    fn startup_subprocess_probe() {
        let Ok(_scenario) = std::env::var(STARTUP_PROBE_SCENARIO) else {
            return;
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("probe runtime should build");

        match runtime.block_on(async_main()) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("Fatal: {error}");
                std::process::exit(2);
            }
        }
    }
}

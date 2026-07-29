//! Protocol constants for vision-core â€” single source of truth.
//!
//! Constants are grouped into two categories, marked clearly below:
//!
//! **[CONSENSUS]** â€” every node on the network must use the same value.
//!   Changing these is a hard fork. Never read from env vars or config files.
//!
//! **[POLICY]**    â€” local node behaviour. Safe to tune without a network
//!   upgrade, though extreme values may degrade interoperability.
//!
//! Nothing in this file may be duplicated anywhere else in the codebase.
//! If you need a constant in another module, import it from here.

// â”€â”€â”€ Identity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Human-readable package release shown in diagnostics.
///
/// This is derived from Cargo package metadata so the runtime banner, status
/// endpoint, and diagnostic handshake tag cannot drift from `Cargo.toml`.
/// It does not control protocol or consensus compatibility.
pub const NODE_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// [CONSENSUS] Wire protocol version. Peers that send a different value are
/// rejected immediately during handshake.
///
/// Version 4 gates the v1.0.3 Alpha/Testnet fork-choice semantics. It is not
/// compatible with v1.0.2 peers, which may reject deep higher-work reorgs.
pub const PROTOCOL_VERSION: u32 = 4;

/// [CONSENSUS] Fork-choice/consensus compatibility version advertised through
/// the P2P protocol version for v1.0.3.
pub const CONSENSUS_VERSION: u32 = 3;

/// [CONSENSUS] Block-header encoding scheme version. Embedded as the first 4
/// bytes of every `BlockHeader::canonical_bytes()` call and therefore part of
/// every PoW hash preimage. Changing this is a hard fork.
pub const BLOCK_VERSION: u32 = 1;

/// [CONSENSUS] Network identifier embedded in every handshake message and used
/// to derive the chain-id bytes. Nodes on a different network string are
/// disconnected.
pub const NETWORK_ID: &str = "mainnet";

// â”€â”€â”€ Ports â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Default HTTP API listen port.
pub const DEFAULT_HTTP_PORT: u16 = 7070;

/// [POLICY] Default P2P listen port used for handshake, block relay, and
/// mining coordination.
pub const DEFAULT_P2P_PORT: u16 = 7072;

// â”€â”€â”€ Block timing & difficulty â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [CONSENSUS] Target inter-block interval in seconds.
/// The difficulty retarget algorithm drives the observed average toward this.
pub const TARGET_BLOCK_TIME: u64 = 30;

/// [CONSENSUS] Number of blocks in one LWMA difficulty retarget window.
/// More-recent intervals receive a proportionally higher weight.
pub const RETARGET_WINDOW: u64 = 20;

/// [CONSENSUS] Absolute minimum difficulty. No block may claim a difficulty
/// below this value; the retarget algorithm is also clamped to this floor.
pub const DIFFICULTY_FLOOR: u64 = 1;

/// [CONSENSUS] Multiplier above `TARGET_BLOCK_TIME` at which the wall-clock
/// stall detector fires and applies an emergency difficulty downshift.
/// At 4Ã— (120 s with a 30 s target) the network is considered stalled.
pub const STALL_MULTIPLIER: u64 = 4;

/// [CONSENSUS] Fraction of difficulty retained after an emergency downshift.
/// 0.75 = 75 %; the remaining 25 % is shed to let miners find the next block.
/// Stored as a float for documentation; the actual calculation uses integer
/// arithmetic: difficulty * 3 / 4 (see `pow::difficulty::calculate_next_difficulty`).
pub const STALL_DOWNSHIFT_FACTOR: f64 = 0.75;

/// [CONSENSUS] Per-interval minimum solve time used in LWMA (seconds).
/// Intervals shorter than this are clamped up to prevent timestamp-reuse attacks.
/// Value = TARGET_BLOCK_TIME / 4 = 7 s.
pub const LWMA_MIN_INTERVAL_SECS: u64 = TARGET_BLOCK_TIME / 4; // 7 s

/// [CONSENSUS] Per-interval maximum solve time used in LWMA (seconds).
/// Intervals longer than this are clamped down to prevent deliberate long-block
/// attacks that would inflate average interval and tank difficulty.
/// Value = TARGET_BLOCK_TIME * 6 = 180 s.
pub const LWMA_MAX_INTERVAL_SECS: u64 = TARGET_BLOCK_TIME * 6; // 180 s

// â”€â”€â”€ Block validation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [CONSENSUS] Blocks with a timestamp more than this many seconds ahead of
/// the local wall clock are rejected as too far in the future.
pub const MAX_FUTURE_TIMESTAMP_SECS: u64 = 7_200; // 2 hours

/// [POLICY] Historical v1.0.2 reorg depth limit.
///
/// v1.0.3 fork choice no longer treats this as canonical validity. A fully
/// validated branch with strictly greater cumulative work is eligible to win
/// regardless of depth. Runtime resource limits must pause/delay recovery,
/// not silently redefine consensus validity.
pub const MAX_REORG: u64 = 36;

/// [POLICY] Historical bootstrap replay budget. This is not a replacement
/// fork-choice cap in v1.0.3.
pub const MAX_REORG_BOOTSTRAP: u64 = 2_048;

/// [POLICY] Historical finality depth retained for diagnostics until explicit
/// deterministic checkpoint/finality semantics are specified.
pub const FINALITY_DEPTH: u64 = 50;

/// [CONSENSUS] Maximum serialised weight units allowed per block.
pub const BLOCK_WEIGHT_LIMIT: u64 = 400_000;

// â”€â”€â”€ Block production (node policy) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Soft target number of transactions per block, used when building
/// the candidate block for mining. Does not affect validation.
pub const BLOCK_TARGET_TXS: usize = 200;

// â”€â”€â”€ Snapshots â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] A full state snapshot is persisted to disk every N blocks.
/// Snapshots speed up reorg recovery; the interval is a storage/speed trade-off.
pub const SNAPSHOT_EVERY: u64 = 32;

// â”€â”€â”€ Tokenomics â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [CONSENSUS] Decimal precision of the native token (fixed-point divisor = 10^9).
pub const TOKEN_DECIMALS: u8 = 9;

/// [CONSENSUS] Block subsidy at genesis in raw token units (decimals included).
/// 510 tokens Ã— 10^9 = 510_000_000_000.
pub const EMISSION_PER_BLOCK: u128 = 510_000_000_000;

/// [CONSENSUS] Blocks between each subsidy halving (~4 years at 30 s/block).
pub const HALVING_INTERVAL: u64 = 2_102_400;

/// [CONSENSUS] Basis points of each block's fees that are burned (10 %).
/// 1_000 bps = 10 %. Remainder goes to the block producer.
pub const FEE_BURN_BPS: u32 = 1_000;

// â”€â”€â”€ Mining gate â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Minimum number of connected peers with fresh heights required
/// before the node is allowed to start (or resume) mining. Prevents solo
/// mining on a partitioned or stalled node.
pub const MIN_PEERS_FOR_MINING: usize = 1;

// â”€â”€â”€ Mempool â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Maximum number of unconfirmed transactions held in memory before
/// the oldest entry is evicted.
pub const MEMPOOL_MAX: usize = 10_000;

/// [POLICY] Maximum transaction-submission requests accepted per second from a
/// single IP address (inbound rate limit).
pub const RATE_SUBMIT_RPS: u64 = 8;

/// [POLICY] Maximum gossip messages accepted per second from a single peer.
pub const RATE_GOSSIP_RPS: u64 = 20;

// â”€â”€â”€ Orphan pool â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Maximum number of orphan blocks held in memory. When the pool
/// exceeds this limit the oldest entry is evicted (FIFO). This bounds memory
/// exposure to missing-parent floods from malicious peers.
pub const ORPHAN_POOL_MAX: usize = 2_000;

// â”€â”€â”€ Peer / P2P â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] A peer's reported height is considered stale if it has not been
/// refreshed within this many seconds. The window must exceed the sync
/// watchdog cadence so inbound-only summary refreshes can be consumed.
pub const PEER_HEIGHT_STALE_SECS: u64 = 45;

/// [POLICY] If a height poll sent to a peer has not received a response within
/// this window (seconds), the peer is treated as height-stalled and excluded
/// from consensus height calculation.
pub const HEIGHT_POLL_RESPONSE_WINDOW_SECS: u64 = 10;

/// [POLICY] Target number of outbound P2P connections to maintain.
pub const TARGET_OUTBOUND_PEERS: usize = 8;

/// [POLICY] Hard cap on total simultaneous P2P connections (inbound + outbound).
pub const MAX_CONNECTIONS: usize = 64;

/// [POLICY] Interval in seconds between gossip heartbeat broadcasts.
/// Too-frequent gossip disrupts active sync sessions (see Fix 10).
pub const GOSSIP_INTERVAL_SECS: u64 = 15;

// â”€â”€â”€ Sync â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Number of blocks the local tip must be behind a peer before the
/// sync watchdog clears the active mining job to prioritise catch-up.
/// Small gaps (< this value) do not interrupt mining (Fix 7).
pub const SYNC_LAG_THRESHOLD: u64 = 5;

/// [POLICY] Alias kept for internal use; same value as SYNC_LAG_THRESHOLD.
pub const SYNC_CLEAR_JOB_MIN_LAG: u64 = SYNC_LAG_THRESHOLD;

/// [POLICY] Timeout per binary-search step during fork detection (seconds).
/// 15 steps Ã— 5 s = 75 s, well within the 120 s outer sync timeout (Fix 13).
pub const SYNC_FORK_SEARCH_TIMEOUT_SECS: u64 = 5;

/// [POLICY] Timeout for the initial tip-hash check at sync start (seconds).
pub const SYNC_FORK_TIMEOUT_SECS: u64 = 10;

/// [POLICY] Batch timeout (seconds) used when the remaining sync gap is small
/// (â‰¤ 2 blocks). Avoids a 30 s wait for a gap that will resolve quickly.
pub const SYNC_SHORT_BATCH_TIMEOUT_SECS: u64 = 5;

/// [POLICY] How long (seconds) to hold the "syncing" gate open after a stall
/// is detected. Must cover the full stall cooldown plus a safety buffer.
pub const STALL_OVERRIDE_SECS: u64 = 120;

// â”€â”€â”€ VisionX PoW â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [CONSENSUS] Size of the VisionX base dataset in megabytes.
pub const VISIONX_DATASET_MB: usize = 256;

/// [CONSENSUS] Per-hash scratchpad size in megabytes.
pub const VISIONX_SCRATCH_MB: usize = 32;

/// [CONSENSUS] Number of mix iterations per hash attempt.
pub const VISIONX_MIX_ITERS: u32 = 65_536;

/// [CONSENSUS] Dependent memory reads per mix iteration.
pub const VISIONX_READS_PER_ITER: u32 = 4;

/// [CONSENSUS] Stride for deterministic dataset write-backs (every N iters).
pub const VISIONX_WRITE_EVERY: u32 = 4;

/// [CONSENSUS] Blocks per VisionX epoch; the full dataset is rebuilt once per
/// epoch. Miners cache the dataset across `clear_job()` calls within an epoch.
pub const VISIONX_EPOCH_BLOCKS: u32 = 32;

// â”€â”€â”€ Seed peers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// [POLICY] Bootstrap peer addresses contacted on first startup. These are
/// never consensus-critical; the network converges on live peers via gossip.
pub const DEFAULT_SEED_PEERS: &[&str] = &[
    "16.163.123.221:7072",
    "69.173.206.211:7072",
    "69.173.207.135:7072",
    "75.128.156.69:7072",
    "98.97.137.74:7072",
    "182.106.66.15:7072",
];

// â”€â”€â”€ Unit tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_version_matches_package_metadata() {
        assert_eq!(NODE_VERSION, concat!("v", env!("CARGO_PKG_VERSION")));
        assert_eq!(env!("CARGO_PKG_VERSION"), "1.0.4");
    }

    #[test]
    fn protocol_version_is_nonzero() {
        assert!(PROTOCOL_VERSION > 0);
    }

    #[test]
    fn ports_are_nonzero_and_distinct() {
        assert!(DEFAULT_HTTP_PORT > 0);
        assert!(DEFAULT_P2P_PORT > 0);
        assert_ne!(DEFAULT_HTTP_PORT, DEFAULT_P2P_PORT);
    }

    #[test]
    fn target_block_time_positive() {
        assert!(TARGET_BLOCK_TIME > 0);
    }

    #[test]
    fn difficulty_floor_is_at_least_one() {
        assert!(DIFFICULTY_FLOOR >= 1, "difficulty must never reach zero");
    }

    #[test]
    fn stall_threshold_exceeds_one_block() {
        // Emergency downshift fires at STALL_MULTIPLIER Ã— TARGET_BLOCK_TIME.
        let stall_secs = STALL_MULTIPLIER * TARGET_BLOCK_TIME;
        assert!(
            stall_secs > TARGET_BLOCK_TIME,
            "stall threshold must be greater than one block time"
        );
    }

    #[test]
    fn future_timestamp_window_is_reasonable() {
        // 2 hours minimum to tolerate clock skew; cap at 24 hours.
        assert!(MAX_FUTURE_TIMESTAMP_SECS >= 7_200);
        assert!(MAX_FUTURE_TIMESTAMP_SECS <= 86_400);
    }

    #[test]
    fn historical_reorg_depth_constants_are_diagnostic_only() {
        assert!(MAX_REORG > 0);
        assert!(MAX_REORG_BOOTSTRAP >= MAX_REORG);
        assert!(FINALITY_DEPTH > 0);
    }

    #[test]
    fn snapshot_interval_aligns_with_visionx_epoch() {
        // Snapshots at epoch boundaries let reorg recovery skip dataset rebuilds.
        assert_eq!(
            SNAPSHOT_EVERY, VISIONX_EPOCH_BLOCKS as u64,
            "SNAPSHOT_EVERY should match VISIONX_EPOCH_BLOCKS"
        );
    }

    #[test]
    fn sync_lag_threshold_matches_alias() {
        assert_eq!(SYNC_LAG_THRESHOLD, SYNC_CLEAR_JOB_MIN_LAG);
    }

    #[test]
    fn peer_summary_freshness_survives_watchdog_cadence() {
        const SYNC_WATCHDOG_INTERVAL_SECS: u64 = 20;
        assert!(PEER_HEIGHT_STALE_SECS > SYNC_WATCHDOG_INTERVAL_SECS);
        assert!(PEER_HEIGHT_STALE_SECS > HEIGHT_POLL_RESPONSE_WINDOW_SECS);
    }

    #[test]
    fn stall_downshift_factor_in_valid_range() {
        assert!(STALL_DOWNSHIFT_FACTOR > 0.0);
        assert!(STALL_DOWNSHIFT_FACTOR < 1.0);
    }

    #[test]
    fn emission_and_halving_are_nonzero() {
        assert!(EMISSION_PER_BLOCK > 0);
        assert!(HALVING_INTERVAL > 0);
    }

    #[test]
    fn seed_peers_are_nonempty_and_use_p2p_port() {
        assert!(!DEFAULT_SEED_PEERS.is_empty());
        for addr in DEFAULT_SEED_PEERS {
            let port_str = format!(":{}", DEFAULT_P2P_PORT);
            assert!(
                addr.ends_with(&port_str),
                "seed peer '{}' should use the default P2P port {}",
                addr,
                DEFAULT_P2P_PORT
            );
        }
    }

    #[test]
    fn network_id_is_nonempty() {
        assert!(!NETWORK_ID.is_empty());
    }

    #[test]
    fn orphan_pool_limit_is_bounded() {
        // Sanity-check: pool large enough to be useful, small enough to be safe.
        assert!(ORPHAN_POOL_MAX >= 100);
        assert!(ORPHAN_POOL_MAX <= 10_000);
    }

    #[test]
    fn min_peers_for_mining_is_at_least_one() {
        assert!(
            MIN_PEERS_FOR_MINING >= 1,
            "solo mining on a partitioned node must not be allowed"
        );
    }
}

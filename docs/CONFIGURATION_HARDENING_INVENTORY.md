# Configuration Hardening Inventory

## Purpose and Scope

This document inventories every configuration source found during the
Vision-Core Phase II Entry Audit. It is maintained to record current promoted
behavior as Configuration Hardening advances. It does not authorize or
implement Configuration Hardening.

The inventory distinguishes:

- operator-controlled runtime configuration;
- build and diagnostic metadata;
- test-only controls;
- compile-time protocol and node-policy constants;
- hard-coded operational controls;
- toolchain and CI configuration.

Consensus and protocol constants are included to make the boundary explicit.
They are not candidates for operator configuration.

## Configuration Source Summary

| Source | Implemented | Location | Notes |
| --- | --- | --- | --- |
| Command-line flags | No | `src/main.rs` | No `std::env::args`, Clap, or equivalent parser exists. |
| Environment variables through `Settings` | Yes | `src/config/settings.rs` | Primary operator configuration. |
| Environment variables outside `Settings` | Yes | `src/main.rs`, `src/node/runtime.rs`, `src/chain/accept.rs` | Logging, runtime threads, and diagnostic identity. |
| Configuration file | No | None | `VISION_CONFIG` exists only in a source comment; no loader or precedence model exists. |
| Default values | Yes | `src/config/settings.rs`, `src/config/constants.rs`, runtime modules | Mix of central constants and hard-coded values. |
| Compile-time protocol constants | Yes | `src/config/constants.rs` and protected modules | Consensus and compatibility sensitive; not runtime configuration. |
| Test-only environment controls | Yes | `src/pow/visionx.rs`, `src/node/bootstrap.rs` | Must remain isolated from production behavior. |
| Cargo and toolchain | Yes | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | Build identity and dependency configuration. |
| GitHub Actions | Yes | `.github/workflows/ci.yml` | Validation environment, triggers, permissions, and commands. |

## Operator Runtime Settings

All fields below are constructed by `Settings::from_env()`, which returns
`Result<Settings, SettingsError>` so invalid configuration can fail before
storage or service initialization.

| Item | Location and consumer | Purpose and current behavior | Startup impact | Runtime impact | Consensus impact | Validation if modified |
| --- | --- | --- | --- | --- | --- | --- |
| `VISION_DATA_DIR` | `src/config/settings.rs`; `node::bootstrap::initialize_chain_state`; `ChainState::open_with_genesis` | Default `./data`. Explicit empty, whitespace-only, padded, inaccessible, unwritable, file-valued, or unusable database locations fail before sled opens; valid relative and absolute paths retain the existing `<value>/chain.db` layout. | High: validates and selects persistent state before services start. | Determines all durable chain and snapshot state for the process. | Persistence-sensitive but does not redefine consensus rules. Selecting the wrong database can expose incompatible state. | `cargo check`; focused settings/startup tests; storage; restart; reorg; snapshot; state-root; full release suite; CI. |
| `VISION_HTTP_PORT` | `src/config/settings.rs`; parsed and bound in `src/main.rs` | Default `7070`. Non-numeric or out-of-range input silently becomes the default. Bind host is hard-coded to `0.0.0.0`. | High: HTTP parsing and bind occur after chain initialization and service construction. Bind failure aborts startup before successful-startup reporting. | Selects API listener exposure. | None directly; API behavior only. | `cargo check`; focused parser/startup/API bind tests; watchdog and VisionX under the configuration policy; full release suite; CI. |
| `VISION_P2P_PORT` | `src/config/settings.rs`; `node::services::start_services` | Default `7072`. Non-numeric or out-of-range input silently becomes the default. Bind host is hard-coded to `0.0.0.0`. | High: parsed after chain initialization. The listener binds before its task is detached; bind failure aborts startup. | Selects inbound P2P listener. | Protocol-adjacent operational input; does not change wire rules. | `cargo check`; focused parser/listener/P2P tests; watchdog; relevant multi-node tests; VisionX; full release suite; CI. |
| `VISION_P2P_ADVERTISED_HOST` | `src/config/settings.rs`; P2P connection and handshake construction | Missing is accepted only with a missing advertised port. Empty or malformed values and host-only configuration fail during configuration loading. | High for explicitly configured public identity because validation occurs before services start. | Affects durable peer identity advertised to peers and whether other nodes accept it. | Network compatibility and reachability sensitive; not block consensus. | Focused parser, handshake, advertised-identity, and private-address tests; watchdog; multi-node; VisionX; full release suite; CI. |
| `VISION_P2P_ADVERTISED_PORT` | `src/config/settings.rs`; P2P handshake construction | Missing is accepted only with a missing advertised host. Invalid, out-of-range, zero, or port-only configuration fails during configuration loading. | High for explicitly configured public identity because validation occurs before services start. | Completes advertised identity and affects peer reachability. | Network compatibility sensitive; not block consensus. | Same as advertised host, including host/port pairing cases. |
| `VISION_ALLOW_PRIVATE_PEERS` | `src/config/settings.rs`; peer advertised-address validation | Missing preserves the established `true` default. Present values must be ASCII case-insensitive `true` or `false`; all others fail configuration loading. | Invalid explicit policy prevents startup before peer services launch. | Controls acceptance of loopback, private, and link-local advertised peer identities. | Network policy; can materially affect connectivity and topology, not block validity. | Focused boolean, peer-identity, handshake, watchdog, and multi-node tests; VisionX; full release suite; CI. |
| `VISION_MINER_ADDRESS` | `src/config/settings.rs`; miner candidate and coinbase construction | Missing or invalid input becomes 64 zeroes. Valid input must be exactly 64 lowercase hexadecimal characters. | No failure today. | When mining is enabled, selects the coinbase recipient and therefore candidate state and state root. | Consensus-adjacent input to locally produced valid blocks; validation rules are unchanged. | Focused parser and mining tests; state-root tests; VisionX; watchdog; full release suite; CI. |
| `VISION_MINING` | `src/config/settings.rs`; `src/main.rs`; `node::services` | Missing defaults to false. Present `1` or case-insensitive `true` enables mining; every other present value disables it. | Controls whether miner services and API state are constructed. | Enables ongoing candidate construction and proof search, gated by peers and recovery state. | Does not change validity rules; changes local block production. | Focused boolean/startup/mining tests; watchdog; VisionX; full release suite; CI. |
| `VISION_MINING_THREADS` | `src/config/settings.rs`; no runtime consumer | Missing or parse failure becomes `0`, documented as logical CPU count. The field is never consumed. | None. | None. | None today. A future mining-worker implementation would be proof-of-work operational behavior, not proof validity. | Owner decision first; focused parser/miner tests; watchdog; VisionX; full release suite; CI if implemented or removed. |
| `VISION_ALPHA_AIRDROP_ENABLED` | `src/config/settings.rs`; `NodeApiState`; API router | Missing defaults to false. Present `1` or case-insensitive `true` enables the development-only route; every other present value disables it. | Controls route registration. | Exposes or removes a local state-mutating development endpoint. | Not a consensus-rule change, but accepted mutations still affect local state through application behavior. | Focused boolean, router, enabled/disabled endpoint, and startup tests; watchdog and VisionX under the configuration policy; full release suite; CI. |
| `VISION_SEED_PEERS` | `src/config/settings.rs`; bootstrap and seed connection loops | Missing uses six compiled defaults. An exactly empty value disables configured seeds. Non-empty input splits on comma, semicolon, or newline; every retained entry must be a socket address. Whitespace-only, delimiter-only, or malformed non-empty input fails configuration loading. | Invalid entries block startup before connection tasks launch; reachability failures do not. | Determines initial outbound connectivity and discovery opportunities. | Networking policy only; advertised chain data remains untrusted. | Focused parser and address tests; seed-loop/P2P tests; watchdog; multi-node; VisionX; full release suite; CI. |

## Environment Variables Outside `Settings`

| Item | Location | Purpose and current behavior | Startup/runtime impact | Consensus impact | Validation if modified |
| --- | --- | --- | --- | --- | --- |
| `RUST_LOG` | `src/main.rs`; `tracing_subscriber::EnvFilter::try_from_default_env` | Standard tracing filter. Missing or invalid input silently uses `info`. | Applied before settings and chain startup. Changes diagnostic volume and possibly timing under heavy logging. | No intended consensus effect. | Focused startup/log-filter tests where feasible; `cargo check`; full release suite for behavior changes; CI. |
| `TOKIO_WORKER_THREADS` | `src/node/runtime.rs` | Missing uses logical CPU count with a platform fallback of 4. A present value must be a positive, unpadded `usize`; zero, malformed, negative, padded, or overflowing input returns a structured startup error before runtime construction. | Critical pre-settings startup input. | No intended consensus effect; concurrency can expose nondeterminism. | Focused runtime-builder subprocess tests for missing, invalid, zero, and valid input; watchdog; VisionX; full release suite; CI. |
| `VISION_BINARY_SHA256` | `src/chain/accept.rs` | Included in proof-of-work failure diagnostics; missing becomes `unknown`. | None until a PoW rejection diagnostic is constructed. | Diagnostic only; must not influence acceptance. | Focused PoW diagnostic test proving acceptance result is unchanged; VisionX; full release suite if logic changes. |
| `VISION_GIT_COMMIT` | `src/chain/accept.rs` | Preferred runtime commit string in PoW failure diagnostics; missing falls back to `GIT_COMMIT`, then `unknown`. | Diagnostic only. | None intended. | Focused diagnostic precedence test; VisionX if code path changes. |
| `GIT_COMMIT` | `src/chain/accept.rs` | Secondary alias for diagnostic commit identity. | Diagnostic only. | None intended. | Same as `VISION_GIT_COMMIT`. |

## Declared but Unimplemented Configuration

| Item | Location | Current status | Impact | Classification and validation |
| --- | --- | --- | --- | --- |
| `VISION_CONFIG` | Comment on `Settings::from_env()` in `src/config/settings.rs`; documentation | No environment read, file open, TOML parse, schema, precedence, reload, or migration behavior exists. `toml` is present as a dependency but no loader consumes it. | None today. Implementing it would add a new startup source capable of changing every runtime setting. | Unreachable/unimplemented. Configuration-file precedence and migration are OD-09 Owner Decision Required. An implementation would require focused file/env precedence tests, startup, storage/restart where data path is involved, networking, watchdog, VisionX, full release suite, and CI. Removing the claim is lower risk but still requires owner disposition and separate dependency review. |

## Test-Only Configuration

These variables are not production operator settings.

| Item | Location | Purpose and current behavior | Risk if modified | Validation |
| --- | --- | --- | --- | --- |
| `VISION_POW_DIAGNOSTIC_BYPASS_DATASET_CACHE` | `src/pow/visionx.rs`, inside `#[cfg(test)]` | Value exactly `1` forces fresh VisionX dataset construction. | Consensus-critical test evidence can become nondeterministic or stop comparing cached and clean paths. | Focused VisionX tests, repeated deterministic runs, full release suite. |
| `VISION_POW_DIAGNOSTIC_FRESH_DATASET` | Same | Alias with identical effect to the bypass variable. | Duplicated test control can drift. | Same as above. |
| `VISION_POW_DIAGNOSTIC_DIGEST_ITERATIONS` | VisionX deterministic digest test | Parses `usize`, defaults to 64 on missing or invalid input. Controls repeated diagnostic iterations only. | Can weaken repeated validation if set too low; zero permits no repetition beyond the initial digest. | Focused VisionX test and documented test invocation. |
| `VISION_BOOTSTRAP_WORKER_DIR` | `src/node/bootstrap.rs`, ignored worker test | Required worker database directory. Parent test supplies it. | Test-only persistence isolation. | Bootstrap/recovery and restart tests. |
| `VISION_BOOTSTRAP_WORKER_OUT` | Same | Required worker result file. | Test orchestration only. | Bootstrap/recovery tests. |
| `VISION_BOOTSTRAP_CHECK_BLOCK_HASH` | Same | Optional block-presence assertion input. | Test oracle only. | Bootstrap/recovery tests. |
| `VISION_BOOTSTRAP_EXPECT_FAIL` | Same | Exact value `1` changes the worker’s expected outcome. | Test oracle only. | Bootstrap/recovery tests. |

## Compile-Time Identity and Consensus Constants

These are configuration in the broad build-time sense, but they are protected
protocol inputs rather than Configuration Hardening candidates.

| Items and values | Location and purpose | Startup/runtime impact | Consensus impact | Validation if modified |
| --- | --- | --- | --- | --- |
| `NODE_VERSION = v${CARGO_PKG_VERSION}` | `src/config/constants.rs`; banner, status, diagnostic node tag | Identifies package release. | No direct consensus effect. | Package identity tests, API/status tests, full release suite, release audit. |
| `PROTOCOL_VERSION = 4`, `CONSENSUS_VERSION = 3`, `BLOCK_VERSION = 1`, `NETWORK_ID = "mainnet"` | Protocol, fork-choice compatibility, header encoding, and network identity | Handshake rejection and canonical bytes. | Consensus/protocol critical. Not operator configurable. | Explicit owner authorization; exact vectors; handshake/multi-node; persistence compatibility where applicable; watchdog; VisionX; full release suite; CI. |
| `TARGET_BLOCK_TIME = 30`, `RETARGET_WINDOW = 20`, `DIFFICULTY_FLOOR = 1`, `STALL_MULTIPLIER = 4`, `LWMA_MIN_INTERVAL_SECS = 7`, `LWMA_MAX_INTERVAL_SECS = 180` | Difficulty calculation | Affects expected difficulty. | Consensus critical. | Difficulty and historical vectors; VisionX; reorg; full release suite; compatibility plan; CI. |
| `STALL_DOWNSHIFT_FACTOR = 0.75` | Documentation mirror of active integer `3 / 4` calculation | Unread by runtime. | Dormant consensus documentation value. | Preserve unless separately authorized; compare exact integer semantics. |
| `MAX_FUTURE_TIMESTAMP_SECS = 7200`, `BLOCK_WEIGHT_LIMIT = 400000` | Block validation | Changes accepted blocks. | Consensus critical. | Exact acceptance/rejection tests; full consensus matrix. |
| `MAX_REORG = 36`, `MAX_REORG_BOOTSTRAP = 2048`, `FINALITY_DEPTH = 50` | Historical/diagnostic fork-choice policy | Unread by current runtime. | Dormant historical policy; must not be made a validity cap accidentally. | Owner decision; reorg, persistence, restart, and compatibility validation. |
| `TOKEN_DECIMALS = 9`, `EMISSION_PER_BLOCK = 510000000000`, `HALVING_INTERVAL = 2102400`, `FEE_BURN_BPS = 1000` | Economics metadata and reward rules | Emission and halving are active; decimals and burn basis points are currently unread. | Consensus critical or dormant consensus economics. | Economics vectors, transaction/block acceptance, state root, reorg, full release suite, activation plan. |
| `VISIONX_DATASET_MB = 256`, `VISIONX_SCRATCH_MB = 32`, `VISIONX_MIX_ITERS = 65536`, `VISIONX_READS_PER_ITER = 4`, `VISIONX_WRITE_EVERY = 4`, `VISIONX_EPOCH_BLOCKS = 32` | VisionX parameter set and handshake fingerprint | Controls mining and verification cost and output. | Consensus critical. | Owner authorization; byte/result equivalence or activation; historical vectors; focused VisionX; mining; handshake; full release suite; CI. |
| Genesis constants and hashes | `src/genesis/genesis.rs`: `GENESIS_HASH`, `ECON_HASH`, height, timestamp, difficulty, nonce, miner, vault declarations | Database and handshake identity. | Chain-identity critical. | OD-07 owner decision, genesis vectors, clean database, persisted database isolation, handshake, full release suite. |
| `VPOW_MAGIC`, `VPOW_VERSION`, nonce offset; `STATE_ROOT_MAGIC`, `STATE_ROOT_VERSION` | Historical proof and state-root canonical encoding modules | Canonical bytes and historical validation. | Consensus critical. | Explicit owner authorization, exact byte vectors, historical validation, state-root/persistence/restart as applicable. |
| Transaction size and fee constants | `src/types/transaction.rs`: `MAX_SERIALIZED_TX_BYTES`, `MIN_CASH_TRANSFER_FEE_LIMIT`, `CASH_TRANSFER_BASE_FEE` | Transaction acceptance and fee execution. | Consensus critical. | Transaction vectors, mempool and block acceptance, state root, full release suite. |

## Compile-Time Node Policy Constants

| Items and values | Purpose/current behavior | Impact and classification | Validation if modified |
| --- | --- | --- | --- |
| `DEFAULT_HTTP_PORT = 7070`, `DEFAULT_P2P_PORT = 7072` | Defaults used by `Settings`. | Startup and exposure policy; not consensus. | Settings/startup/API or P2P tests, watchdog for P2P, full release suite. |
| `BLOCK_TARGET_TXS = 200` | Candidate transaction selection target. | Mining policy; does not change block validity. | Mempool/miner tests, VisionX, full release suite. |
| `SNAPSHOT_EVERY = 32` | Snapshot persistence cadence. | Persistence and recovery policy. | Storage, restart, reorg, snapshot, state-root, full release suite. |
| `MIN_PEERS_FOR_MINING = 1` | Pauses mining without a connected peer. | Mining/network policy. | Mining, peer, watchdog, VisionX, full release suite. |
| `MEMPOOL_MAX = 10000`, `ORPHAN_POOL_MAX = 2000` | In-memory resource bounds. | Node policy; extreme values affect availability, not validity. | Focused pool tests, affected network tests, full release suite. |
| `PEER_HEIGHT_STALE_SECS = 45`, `HEIGHT_POLL_RESPONSE_WINDOW_SECS = 10` | Peer-summary freshness and stall qualification. | Synchronization/recovery policy with indirect consensus risk. | P2P, watchdog, deterministic timing, multi-node, VisionX, full release suite. |
| `SYNC_LAG_THRESHOLD = 5`, `SYNC_CLEAR_JOB_MIN_LAG = alias`, `STALL_OVERRIDE_SECS = 120` | Mining-pause and watchdog recovery policy. | Synchronization/mining coordination. | Watchdog, P2P sync, mining, VisionX, full release suite. |
| `RATE_SUBMIT_RPS = 8`, `RATE_GOSSIP_RPS = 20` | Declared inbound rate limits. | Unused; no runtime effect. Near-term API/security policy. | Owner disposition; API/P2P tests if implemented, full release suite. |
| `TARGET_OUTBOUND_PEERS = 8`, `MAX_CONNECTIONS = 64`, `GOSSIP_INTERVAL_SECS = 15` | Declared connection and gossip policy. | Unused; actual seed loops and heartbeat timings are hard-coded elsewhere. | Owner disposition; P2P, watchdog, multi-node, full release suite. |
| `SYNC_FORK_SEARCH_TIMEOUT_SECS = 5`, `SYNC_FORK_TIMEOUT_SECS = 10`, `SYNC_SHORT_BATCH_TIMEOUT_SECS = 5` | Declared sync timeout policy. | Unused; active receive timeouts are hard-coded. | Owner disposition; deterministic sync/watchdog tests, multi-node, full release suite. |

## Hard-Coded Operational Controls

These values are active configuration embedded outside the central constants
module.

| Value | Location | Purpose/current behavior | Classification | Validation if modified |
| --- | --- | --- | --- | --- |
| `0.0.0.0` for HTTP and P2P | `src/config/settings.rs` | Forces all-interface binds; no bind-host setting exists. | Duplicated hard-coded exposure policy; difficult to change independently from port parsing. | Startup/API/P2P bind tests, watchdog, full release suite. |
| Watchdog interval `20 s` | `src/node/services.rs` | Runs synchronization watchdog. A separate test-only constant with the same value appears in `config/constants.rs`. | Active and duplicated across runtime/test assumption. | Focused watchdog with deterministic time controls, P2P sync, VisionX, full release suite. |
| Mining poll interval `250 ms` | `src/node/services.rs` | Rebuilds or advances mining work. | Active mining policy. | Mining tests, watchdog, VisionX, full release suite. |
| Seed reconnect delay `2 s` | `src/node/services.rs` | Delay after seed connection failure/disconnect. | Active P2P operational policy. | Seed-loop/P2P tests, watchdog, full release suite. |
| Seed heartbeat delay `5 s` | `src/node/services.rs` | Ping cadence on seed connection. | Active P2P operational policy; differs from unused `GOSSIP_INTERVAL_SECS = 15`. | P2P/keepalive/watchdog tests, full release suite. |
| Multiple receive timeouts `5 s` and height-loop bound `8` | `src/node/services.rs`, `src/p2p/connection.rs`, `src/p2p/sync.rs` | Handshake, height, keepalive, and sync receive bounds. | Repeated literals with different meanings; difficult to validate or tune as a unit. Some declared timeout constants are unused. | Per-path focused tests, watchdog, multi-node, VisionX, full release suite. |
| `MAX_MESSAGE_BYTES = 16 MiB` | `src/p2p/connection.rs` | Rejects oversized P2P frames. | Protocol/resource safety, not negotiated. | Framing boundary tests, P2P, watchdog, full release suite. |
| Inbound summary refresh bound `8` | `src/p2p/connection.rs` | Limits messages inspected while refreshing inbound summary. | P2P synchronization policy. | Focused inbound refresh and watchdog tests. |
| VisionX dataset cache capacity `3` | `src/pow/visionx.rs` | Bounds cached datasets. | Performance/resource policy; result must remain identical. | Cache eviction, deterministic VisionX, full release suite. |

## Build, Toolchain, and CI Configuration

| Item | Location | Current behavior | Impact | Validation if modified |
| --- | --- | --- | --- | --- |
| Package version `1.0.4`, edition `2021`, binary path | `Cargo.toml` | Defines package/runtime identity and compilation edition. | Release identity; edition changes can affect compilation/lints. | Separate dependency/identity review, check, full release suite, CI, release audit. |
| Locked dependencies | `Cargo.toml`, `Cargo.lock` | Cargo resolves locked dependency graph. `toml = 0.8` is present without a config loader. | Supply chain, serialization, crypto, runtime, and platform behavior. | Separate dependency commit; check, focused affected tests, VisionX where relevant, full release suite, CI. |
| Rust `1.97.1`, minimal profile, Clippy and rustfmt | `rust-toolchain.toml` | Pinned validated toolchain. | Compiler, formatting, diagnostics, and generated behavior. | Separate toolchain commit; warning comparison; check; formatting; full release suite; VisionX; CI. |
| CI triggers | `.github/workflows/ci.yml` | Pushes to `main` and PRs targeting `main`. | Determines remote validation coverage. | Workflow syntax and GitHub run; no source behavior. |
| CI permissions | Same | `contents: read`. | Least-privilege repository access. | GitHub run and permission review. |
| `CARGO_TERM_COLOR=always`, `CARGO_INCREMENTAL=0` | Same | Stable colored logs and disabled incremental compilation. | CI output/build behavior only. | GitHub run. |
| Blocking check, release test, formatting; non-blocking Clippy | Same | Uses Windows latest, pinned repository toolchain, locked dependencies, and single-threaded release tests. | Promotion evidence. | GitHub run; policy review for any change. |

## Test Construction Configuration

Several tests construct `Settings` directly rather than using environment
parsing:

- `src/api/alpha.rs`;
- `src/api/read_only.rs`;
- `src/node/bootstrap.rs`;
- `src/tests/multi_node.rs`.

This isolates tests from the process environment but duplicates every
`Settings` field. Adding or changing a field requires updating all literals.
The settings module now also has source-neutral parser tests and subprocess
startup probes for hardened configuration boundaries.

## Dependency and Quality Classification

### Duplicated

- HTTP and P2P bind host `0.0.0.0` is repeated.
- Watchdog cadence `20 s` is active in services and repeated as a test-local
  constant.
- Multiple unrelated network operations use unexplained `5 s` literals while
  similarly named central sync-timeout constants are unused.
- `VISION_POW_DIAGNOSTIC_BYPASS_DATASET_CACHE` and
  `VISION_POW_DIAGNOSTIC_FRESH_DATASET` are aliases for the same test behavior.
- `VISION_GIT_COMMIT` and `GIT_COMMIT` are intentional diagnostic aliases but
  need documented precedence.
- Direct `Settings` literals are repeated in four test areas.

### Obsolete or misleading

- The `VISION_CONFIG` source comment describes a loader that does not exist.
- `toml` is present as a dependency but has no current configuration consumer.
- `VISION_MINING_THREADS` describes logical-CPU behavior but has no consumer.
- Several policy constants describe features not wired into runtime, as
  classified in `DEAD_CODE_LEDGER.md`.

### Inconsistent

- Invalid ports still silently use defaults; mining and alpha booleans still
  become false for unrecognized input; invalid miner addresses still become
  the zero address. P2P identity, private-peer policy, and seed syntax now fail
  during configuration loading.
- `TOKIO_WORKER_THREADS` is active while `VISION_MINING_THREADS` is unused;
  their names can be mistaken as controlling the same concurrency.
- HTTP bind still occurs after service construction, but failure now prevents
  successful-startup reporting and terminates startup.

### Unreachable or unused

- Command-line configuration is absent.
- Configuration-file loading and `VISION_CONFIG` are unreachable.
- `Settings::mining_threads` is unused.
- Declared rate, connection, gossip, and fork-timeout policy constants listed
  above are unused by runtime.

### Difficult to validate

- `Settings::default()` still reads global process environment directly for
  default construction, while `Settings::from_env()` exposes structured
  configuration failure to startup.
- Settings acquisition is separated from typed parsing and covered by
  source-neutral and subprocess characterization tests.
- Runtime construction happens before `Settings`, so runtime-thread validation
  has a separate failure boundary.
- Startup side effects are not staged: HTTP bind failure can occur after P2P
  and mining tasks are launched.
- Seed-peer reachability errors remain asynchronous, while syntax errors now
  fail during configuration loading.
- Several network timeout literals have distinct semantics but no typed names.

## Inventory Conclusions

Configuration Hardening can begin without changing consensus, but it cannot be
treated as parsing cleanup. It changes startup failure behavior, networking
topology, API exposure, mining behavior, and persistence selection.

The safest initial work is characterization and a source-neutral parsing seam.
The following decisions must be made before their dependent phases:

- define when a miner address is required;
- implement or remove `VISION_MINING_THREADS`;
- implement or remove `VISION_CONFIG`, including precedence;
- preserve the implemented data-directory and empty seed-list policies;
- decide whether hard-coded bind hosts become operator settings;
- decide whether ancillary variables such as `RUST_LOG` and
  `TOKIO_WORKER_THREADS` are inside the strict-validation contract.

No consensus, genesis, protocol-version, VisionX, canonical serialization, or
persistence-format value should become runtime configurable during this work.

# Vision-Core

Vision-Core is the Rust blockchain node for the Vision network. The current
authoritative release is `vision-core-consensus-v1.0.4`.

The node has a validated consensus release suite, but its developer-facing APIs,
configuration, and operational tooling remain under active development. Do not
assume that the HTTP API or environment-variable behavior is stable until a
stability policy is published.

## Prerequisites

The currently validated environment is:

- Windows on `x86_64-pc-windows-msvc`;
- Rust 1.97.1, as declared in `rust-toolchain.toml`;
- Visual Studio Build Tools with the MSVC C/C++ toolchain and Windows SDK;
- Cargo and Git.

Other operating systems have not been established as release-validated by the
repository evidence. They may work, but no compatibility claim is made here.

Cargo can resolve and download dependencies online. Once every locked dependency
is present in the local Cargo cache, the validated commands can be run with
`--offline`. Always retain `--locked` so Cargo uses `Cargo.lock`.

## Build

```powershell
cargo build --release --locked
```

For a cached, offline build:

```powershell
cargo build --release --locked --offline
```

The executable is `target/release/vision-core.exe` on Windows.

## Run

Vision-Core currently accepts configuration through environment variables; it
does not expose command-line flags.

```powershell
$env:VISION_DATA_DIR = "C:\vision-data"
cargo run --release --locked
```

The node initializes tracing, loads settings, opens or restores chain state,
starts P2P, synchronization, and optional mining services, then serves the HTTP
API. See [Architecture](docs/ARCHITECTURE.md) and
[Configuration](docs/CONFIGURATION.md).

## Configuration

Supported variables include:

| Variable | Type | Default | Currently consumed |
| --- | --- | --- | --- |
| `VISION_DATA_DIR` | path string | `./data` | Yes |
| `VISION_HTTP_PORT` | `u16` | `7070` | Yes |
| `VISION_P2P_PORT` | `u16` | `7072` | Yes |
| `VISION_P2P_ADVERTISED_HOST` | non-empty string | unset | Yes |
| `VISION_P2P_ADVERTISED_PORT` | nonzero `u16` | unset | Yes |
| `VISION_ALLOW_PRIVATE_PEERS` | boolean-like string | `true` | Yes |
| `VISION_MINER_ADDRESS` | 64 lowercase hex characters | 64 zeroes | Yes |
| `VISION_MINING` | boolean-like string | `false` | Yes |
| `VISION_MINING_THREADS` | unsigned integer | `0` | Parsed, currently unused |
| `VISION_ALPHA_AIRDROP_ENABLED` | boolean-like string | `false` | Yes |
| `VISION_SEED_PEERS` | delimited address list | compiled defaults | Yes |

Invalid numeric and boolean values currently fall back silently; an invalid
miner address becomes the zero address. `VISION_CONFIG` is mentioned in source
documentation but is not presently read. Exact behavior and examples are in
[Configuration](docs/CONFIGURATION.md).

## HTTP API

The default listener is `0.0.0.0:7070`.

| Method | Route | Purpose |
| --- | --- | --- |
| `GET` | `/status` | Current node, chain, peer, mining, and recovery snapshot |
| `GET` | `/peers` | Peer list; currently returns an empty stub |
| `GET` | `/balance/:address` | Canonical balance snapshot |
| `GET` | `/nonce/:address` | Canonical nonce snapshot |
| `GET` | `/transaction/:txid` | Canonical or mempool transaction lookup |
| `GET` | `/mining/info` | Mining and recovery state |
| `POST` | `/transactions` | Submit a canonical signed transaction JSON object |
| `POST` | `/alpha/airdrop` | Development-only funding endpoint, conditionally registered |

The API is not yet declared stable. Request and response contracts, including
known inconsistencies, are documented in [HTTP API](docs/API.md).

## Test and validation

Authoritative release validation is single-threaded:

```powershell
cargo test --release --locked -- --test-threads=1
```

Single-threading is retained because parts of the suite use shared process
resources, fixed network assumptions, global caches, and environment-sensitive
state. It is the established release-validation mode.

Useful narrower commands:

```powershell
cargo test --release --locked p2p::sync::tests:: -- --test-threads=1
cargo test --release --locked p2p::sync::tests::watchdog_recovers_with_valid_peer_after_malicious_peer_fails -- --exact --test-threads=1
cargo test --release --locked pow::visionx::tests:: -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --locked
```

At the v1.0.4 baseline, one test,
`node::bootstrap::tests::bootstrap_recovery_worker`, is intentionally ignored.
Formatting and Clippy are known debt checks and are non-blocking in the initial
CI baseline; their failures remain visible.

## Continuous integration

CI runs for pushes to `main` and pull requests targeting `main`.

- Build/check and the single-threaded release suite are blocking.
- Formatting and Clippy run as explicitly named non-blocking debt-reporting
  jobs until Tranche 2 establishes clean baselines.

## Consensus-sensitive changes

Changes involving block encoding, PoW, VisionX, genesis, transaction execution,
chain acceptance, state-root calculation, or P2P wire types require isolated
commits, characterization or golden-vector tests, and the complete release
suite. See the [Consensus Change Policy](docs/CONSENSUS_CHANGE_POLICY.md).

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) before preparing a change. Security
reports should follow [SECURITY.md](SECURITY.md).

The repository does not currently contain an authoritative license. See
[License Decision Required](docs/LICENSE_DECISION_REQUIRED.md).

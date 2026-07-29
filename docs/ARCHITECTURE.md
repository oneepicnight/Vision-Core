# Vision-Core Architecture

This document describes the v1.0.4 source tree as implemented. It does not
describe planned functionality.

## Executable and startup

`src/main.rs` is the only Cargo binary entry point. Startup proceeds as follows:

1. Build the Tokio runtime in `node::runtime`.
2. Initialize `tracing-subscriber` from `RUST_LOG`, defaulting to `info`.
3. Construct `Settings` from environment variables.
4. Print the startup banner.
5. Open chain state and run bootstrap/recovery initialization.
6. Create shared chain, peer-manager, mempool, optional miner-manager, and
   recovery-state objects.
7. Load compiled or configured seed peers into the peer manager.
8. Start P2P, synchronization, keepalive, and optional mining services.
9. Construct HTTP API state and serve the Axum router.

The HTTP server is the final awaited service. Startup errors propagate to
`main`, are logged as fatal, and terminate the process with status 1.

## Module ownership

| Module | Responsibility | Classification |
| --- | --- | --- |
| `api` | Axum routes, response DTOs, and read-only/runtime state adapters | Application-level |
| `chain::accept` | Unified block validation and acceptance path | Consensus-critical |
| `chain::state_root` | Canonical state-vector encoding and root computation | Consensus-critical |
| `chain::reorg` | Branch reconstruction and cumulative-work reorganization | Consensus-critical / adjacent |
| `chain::storage` | Sled serialization, canonical indexes, and tip metadata | Consensus-adjacent persistence |
| `chain::snapshots` | State snapshot persistence and recovery | Consensus-adjacent persistence |
| `chain::orphan` | Unknown-parent block holding and promotion | Consensus-adjacent |
| `config::constants` | Protocol, consensus, policy, and runtime constants | Mixed; labels in source govern |
| `config::settings` | Environment-derived runtime settings | Application-level |
| `genesis` | Genesis block, locked hashes, and genesis verification | Consensus-critical |
| `mempool` | Admission policy and pending transaction storage | Policy / consensus-adjacent |
| `miner` | Candidate construction, job state, and block submission | Consensus-adjacent |
| `node` | Bootstrap, runtime wiring, services, and recovery state | Application orchestration / adjacent |
| `p2p::messages` | Serialized wire-message enum | Protocol-critical |
| `p2p::protocol` | Handshake identity, compatibility, and announcements | Protocol-critical |
| `p2p::connection` | Framing and inbound/outbound connection behavior | Protocol-adjacent |
| `p2p::sync` | Peer selection, block download, watchdog recovery | Consensus-adjacent |
| `p2p::peer_manager` | Peer lifecycle and chain-summary tracking | Application / protocol-adjacent |
| `pow::difficulty` | Difficulty calculation and target comparison | Consensus-critical |
| `pow::historical_vpow` | Historical PoW preimage compatibility | Consensus-critical |
| `pow::visionx` | VisionX hashing and dataset behavior | Consensus-critical |
| `pow::visionx_miner` | VisionX mining jobs and solutions | Consensus-adjacent |
| `types` | Canonical blocks, headers, transactions, and execution | Consensus-critical |
| `tests` and inline `#[cfg(test)]` modules | Unit, integration-style, and multi-node validation | Test infrastructure |

## Data flow

### Incoming block

P2P framing decodes a message, protocol handling identifies a block, and the
block enters the single chain-acceptance path. Acceptance validates structure,
parentage, timestamp, difficulty, PoW, transactions, and state root before
classifying the result as a canonical extension, side-chain block, orphan, or
rejection. Canonical changes are persisted and may trigger orphan promotion or
reorganization.

### Synchronization

The peer manager records peer summaries and connection state. The watchdog
compares local and remote summaries, selects a deterministic eligible target,
downloads blocks, and imports them through the normal acceptance path. A guard
prevents overlapping sync attempts. Higher-work recovery can pause mining while
the node resolves the remote chain.

### Transaction submission

`POST /transactions` decodes a `Tx`, performs mempool admission using canonical
chain nonce and pending transactions, then inserts or replaces a mempool entry.
Candidate construction selects pending transactions and simulates execution.
Block acceptance independently validates execution.

### Mining

When enabled, the service layer builds a candidate for the current tip, computes
the expected state root, creates a VisionX job, and searches nonces. A solution
returns through the same block-acceptance path used for peer blocks. Mining is
gated or paused based on peer and recovery state.

## Storage layout

Vision-Core uses sled beneath `VISION_DATA_DIR`. Storage code defines these
logical key families:

- `block:<hash>`: bincode-serialized block;
- `height:<height>`: canonical block hash at a height;
- `meta:tip_hash` and `meta:tip_height`: canonical tip pointers;
- snapshot keys under `snap:*` for balances, nonces, roots, and snapshot
  metadata.

Peer persistence also has a newline-delimited `PeerStore` implementation, but
the v1.0.4 compiler reports that implementation as unused; it is not documented
as an active persistence guarantee.

Changing encodings, keys, write ordering, or recovery interpretation is outside
developer-foundation work and requires dedicated persistence and compatibility
review.

## Configuration and API

Runtime configuration is currently environment-only. See
`docs/CONFIGURATION.md`. The HTTP router and current contract are documented in
`docs/API.md`.

## Test organization

- Most modules contain unit tests beside implementation.
- `src/tests/single_node.rs` covers basic node/genesis behavior.
- `src/tests/sync.rs` covers synchronization behavior.
- `src/tests/mining.rs` covers mining behavior.
- `src/tests/reorg.rs` covers reorganization behavior.
- `src/tests/multi_node.rs` provides integration-style multi-node scenarios.

The authoritative release suite runs all tests in release mode with one test
thread. Focused P2P watchdog and VisionX commands are listed in the README.

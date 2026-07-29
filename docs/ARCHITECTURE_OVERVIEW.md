# Architecture Overview

## Vision Is a Blockchain Platform

Vision is a blockchain platform first. It is not a game and its protocol is not a game engine.

The blockchain exists to provide decentralized ownership, peer-to-peer exchange, and long-term digital permanence. It establishes a shared history that does not depend on one publisher, marketplace, hosting provider, or game remaining in business. Applications consume that foundation; they do not define it.

Gaming is Vision’s flagship application domain because games make the value of durable identity, scarce assets, direct exchange, creator participation, and persistent worlds tangible. The intended ecosystem also includes wallets, marketplaces, decentralized identity, creator economies, and other applications whose ownership records must survive any individual client.

The architectural order is:

1. Vision-Core establishes and enforces the blockchain protocol.
2. User-facing and developer-facing systems consume Vision-Core services.
3. Product layers provide specialized experiences without becoming consensus authorities.

For project intent, see [PROJECT_VISION.md](PROJECT_VISION.md). This document distinguishes implemented architecture from planned architecture.

## Ecosystem Map

| Component | Status | Responsibility |
| --- | --- | --- |
| Vision-Core | Implemented | Authoritative blockchain node and protocol implementation |
| Vision Desktop | Separate application; current node-manager foundation | User-facing Core lifecycle, local configuration, monitoring, reports, and diagnostics |
| Vision Wallet | Planned | Key custody, signing, balances, transaction construction, and account experience |
| Vision Exchange | Planned | Peer-to-peer asset discovery and exchange services |
| Vision Marketplace | Planned | Listings, discovery, provenance, and creator-facing commerce |
| Vision Land Registry | Planned | Durable ownership records for virtual land and related rights |
| Vision Gaming Layer | Planned | Game-facing ownership, inventory, identity, and settlement integrations |
| Developer SDK | Planned | Supported client libraries and application integration tools |

Only Vision-Core defines current blockchain behavior. The planned components consume validated chain state and submit signed transactions. They do not independently decide whether a block, transaction, or chain is valid.

## Vision-Core

Vision-Core is the authoritative Rust implementation of the Vision blockchain. The current repository builds a node binary that owns:

- consensus and protocol enforcement;
- canonical block, header, and transaction representations;
- proof-of-work and VisionX validation;
- transaction validation and deterministic state execution;
- cumulative-work fork choice and reorganization;
- P2P connectivity, peer compatibility, and synchronization;
- mempool policy;
- mining job construction and solution submission;
- persistent block, index, tip, and snapshot storage;
- restart, replay, and recovery;
- the HTTP boundary used by local and external applications.

`src/main.rs` assembles the runtime around shared chain state, mempool, peer manager, P2P connection manager, optional miner manager, recovery state, and Axum API state. Detailed implemented module behavior remains documented in [ARCHITECTURE.md](ARCHITECTURE.md).

Vision-Core is currently a binary crate. Public Rust re-exports are not a declared third-party library API. That policy remains an owner decision.

## Vision Desktop

Vision Desktop is maintained separately from Vision-Core. Its current repository describes it as a user-facing node manager that launches and controls a verified bundled Vision-Core executable. It owns UI, Core process lifecycle, local configuration, installer and updater behavior, reports, and diagnostics.

Vision Desktop must never duplicate consensus logic. It may present or orchestrate behavior exposed by Vision-Core, but Core remains responsible for block validation, proof validation, state execution, persistence, replay, mining rules, and P2P protocol enforcement.

Planned Desktop responsibilities include:

- wallet workflows;
- node installation, startup, shutdown, and upgrades;
- blockchain and synchronization monitoring;
- configuration and developer tools;
- interfaces for future staking or governance systems if those systems are designed;
- exchange and marketplace interfaces;
- access to future gaming services.

These are planned product capabilities, not claims about the current Desktop release or current Vision-Core protocol.

## Runtime and Consensus Flow

### Transactions and mempool

A transaction has canonical signed fields and a canonical identifier. Stateless validation checks structure, supported method, transfer arguments, size, fee floor, sender encoding, and Ed25519 signature. Stateful execution checks nonce and balance and applies deterministic account changes.

The mempool performs node-local admission and replacement. Mempool acceptance is not consensus acceptance. Every transaction included in a block is validated and executed again through block acceptance.

### Unified block acceptance

`chain::accept::apply_block` is the common acceptance boundary for blocks obtained from peers, synchronization, orphan promotion, and local mining. It classifies a block as rejected, a canonical extension, a valid side-chain block, or an orphan.

The path validates identity, duplication, parentage, height, timestamp, difficulty, VisionX proof of work, coinbase placement and reward, transaction root, transaction execution, state root, persistence effects, and possible reorganization. Rejection must not partially commit state.

### Fork choice

Vision-Core compares valid competing branches by cumulative work, not height alone. Advertised peer work is discovery information, never authority. A node downloads and validates the branch before it can affect the canonical tip.

Reorganization reconstructs state from a common ancestor, replays the candidate branch, checks the resulting state commitment, persists the new canonical indexes, and recovers displaced transactions as appropriate.

## Networking Architecture

### Transport and compatibility

Vision-Core uses framed TCP P2P messages. The handshake exchanges compatibility identity including protocol version, chain ID, genesis hash, network ID, node nonce, height, economics identity, VisionX parameter fingerprint, and peer-discovery information.

Peers with incompatible chain, genesis, protocol, economics, or proof-of-work identities are rejected. Network messages are untrusted input and are bounded and validated before use.

### Peer discovery

Configured seed peers and peers learned through compatible connections populate the peer manager. The manager tracks direction, connection state, advertised identity, height, cumulative work, and freshness. Peer discovery expands reachability; it does not confer trust in advertised chain data.

Persistent peer-store ownership is not yet settled and remains classified in [DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md).

### Synchronization

Synchronization selects eligible peers deterministically, prevents overlapping sync activity, requests missing blocks, and imports every received block through the unified acceptance path. A higher-work recovery operation can pause local mining while the node validates the candidate history.

### Watchdog recovery

The synchronization watchdog detects stalled progress and permits recovery through another eligible peer. The v1.0.4 regression work made the malicious-peer-then-valid-peer test sequence deterministic without changing production peer selection or protocol behavior.

### Why networking is consensus-sensitive

P2P transport is not itself the block-validity function, but networking is consensus-sensitive in operation because it supplies the histories that the validator sees. A synchronization defect can prevent discovery of a superior chain, apply blocks through an incomplete path, mishandle ordering, or allow untrusted advertised work to influence state.

Networking changes therefore require more than transport tests. They must preserve:

- validation of every imported block;
- cumulative-work fork choice;
- deterministic synchronization state transitions;
- safe behavior under malformed, stale, malicious, or stalled peers;
- mining pause and recovery coordination;
- compatibility identity.

Tests deliberately control peer order and timing where required. Production consensus results must not depend on scheduler order or network arrival order.

## Storage, Persistence, and Recovery

Vision-Core uses sled for durable node state. Current storage includes serialized blocks keyed by hash, canonical height-to-hash indexes, tip metadata, snapshot metadata and state-root commitments, and snapshot account state.

`ChainState` is an in-memory model rather than a serialized struct. Startup:

1. opens the configured database;
2. verifies genesis and network identity;
3. loads and validates an applicable canonical snapshot;
4. replays the persisted canonical tail;
5. rebuilds indexes and cumulative work;
6. exposes the reconstructed state to networking, mining, and API services.

### State roots

State roots commit deterministic account state to a block. They prevent a node from accepting a block whose transactions produce a different state from the committed result. State-root inputs, ordering, and encoding are consensus-sensitive.

### Snapshots

Snapshots accelerate restart and recovery but do not replace canonical authority. Snapshot lineage and state-root commitments must agree with canonical storage. Invalid or incompatible snapshot material must not become trusted state.

### Database format stability

Database stability is critical because a node must reconstruct the same canonical state after a restart or software upgrade. Changes to keys, serialized shapes, canonical indexes, snapshot interpretation, replay order, or cumulative-work metadata can strand existing nodes or silently change state.

Database and state-model changes therefore require explicit compatibility analysis, restart validation, reorganization validation, and snapshot/state-root validation. Compiler-reported dead fields are not sufficient evidence for a persistence change.

## Mining and VisionX

Vision-Core implements proof-of-work mining through VisionX. Mining is coupled to consensus validation but is not allowed to bypass it.

### Mining jobs and block assembly

The miner:

1. reads the canonical tip;
2. selects eligible mempool transactions;
3. constructs coinbase and candidate block contents;
4. executes candidate transactions to derive the expected state root;
5. creates a VisionX mining job using current consensus parameters;
6. searches nonce space;
7. submits a found block through ordinary block acceptance.

### Validation flow

Every locally mined or remotely received block is checked against the required difficulty and VisionX proof. The acceptance path revalidates proof of work even if another subsystem previously examined it.

### Historical consensus preservation

Vision-Core retains historical VPoW preimage behavior and exact compatibility vectors. Historical encodings, target semantics, VisionX parameters, dataset derivation, and proof comparison are consensus behavior. A cleaner or faster implementation is acceptable only when it produces identical results for the history it governs.

VisionX caches are performance mechanisms, not alternate sources of truth. Clean computation and cache reuse must yield identical hashes and validation outcomes.

## Application and API Boundary

The Axum API exposes current node status, balance, nonce, transaction lookup and submission, and mining information, plus an explicitly development-only alpha airdrop. The API has known stability, versioning, error-envelope, authentication, and TLS decisions still outstanding.

Applications must treat the API as access to Core, not as a way to redefine Core. Mutating requests enter existing mempool or state paths; user interfaces do not directly alter canonical storage.

## Planned Ecosystem Components

The following architecture is planned and must be designed through separate specifications and decision records.

### Vision Exchange

Vision Exchange is intended to provide peer-to-peer discovery and exchange of blockchain-recognized assets. It may supply order discovery, negotiation, settlement orchestration, and user interfaces. It must submit valid transactions to Vision-Core and must not become the authority for ownership.

### Vision Wallet

Vision Wallet is intended to own key management, signing, account presentation, transaction construction, and recovery workflows. Private keys should not be delegated to consensus services. The wallet observes validated state and submits signed requests.

### Vision Marketplace

Vision Marketplace is intended to support listings, provenance, creator commerce, and asset discovery. Search indexes and listing services may improve usability but remain derived views rather than consensus state unless specific on-chain primitives are separately designed.

### Vision Land Registry

Vision Land Registry is intended to represent durable virtual-land ownership and transfers. The meaning of land rights, issuance, subdivision, governance, and application enforcement requires future protocol and legal-product design.

### Vision Gaming Layer

The Gaming Layer is intended to connect games to identity, ownership, inventories, provenance, and settlement. Rendering, simulation, and cloud execution are not assumed to run in Vision-Core. Games consume blockchain commitments and services while retaining their application-specific execution.

### Developer SDK

The Developer SDK is intended to provide supported interfaces for wallets, games, exchanges, marketplaces, and tools. It should encode documented API contracts and transaction formats without independently implementing consensus.

## End-to-End Data Flow

```text
User
  |
  v
Wallet or application
  |  constructs and signs
  v
Vision-Core API
  |  stateless/state-aware admission
  v
Local mempool
  |  transaction gossip
  +---------------------> Peer network mempools
  |
  | selected for a candidate
  v
Mining and block assembly
  |  VisionX proof search
  v
Unified block acceptance
  |  PoW + transaction + state-root validation
  v
Cumulative-work fork choice
  |  canonical state transition
  v
Persistent blocks, indexes, tip, and snapshots
  |  validated reads and events
  v
Vision Desktop, wallet, exchange, marketplace, games, SDK consumers
```

At no point does a wallet, miner, peer advertisement, indexer, Desktop interface, or planned ecosystem service become consensus authority. Authority comes from deterministic validation of the protocol and the selected cumulative-work chain.

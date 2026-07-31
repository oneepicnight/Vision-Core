# Consensus Boundaries

## Purpose

Consensus is the highest engineering priority in Vision-Core. A consensus defect can cause honest nodes to accept different histories, invalidate existing blockchain data, corrupt durable state, or split the network.

The components in this document are not ordinary application code. Any change that can alter block acceptance, transaction validity, proof-of-work verification, serialization, state calculation, persistence interpretation, or network compatibility requires explicit repository-owner approval and expanded validation before implementation.

This document identifies protected boundaries. It does not authorize changes to them and is not a complete protocol specification. The controlling process policy is [CONSENSUS_CHANGE_POLICY.md](CONSENSUS_CHANGE_POLICY.md); implemented architecture is mapped in [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md).

## Classification Rule

Two honest compatible nodes given the same valid history must derive the same accepted chain and committed state. Anything capable of changing that result is consensus-sensitive.

A refactor is not automatically non-consensus. If emitted bytes, accepted inputs, arithmetic, ordering, historical interpretation, state transitions, or fork choice can change, the work remains consensus-sensitive.

When evidence is incomplete, classify the work as consensus-sensitive until byte-level, state-level, and historical evidence proves otherwise.

## Consensus Rules

### Block acceptance

`chain::accept::apply_block` is the unified acceptance boundary for blocks received through synchronization, peer propagation, orphan promotion, and local mining. It determines whether a block is rejected, becomes a canonical extension, is stored as a valid side-chain block, or remains an orphan.

Protected behavior includes:

- duplicate and identity checks;
- parent and height validation;
- timestamp and difficulty checks;
- proof-of-work verification;
- coinbase placement and reward validation;
- transaction-root verification;
- transaction execution;
- state-root verification;
- atomic persistence;
- reorganization eligibility.

No source may create a shortcut that allows one block origin to bypass checks applied to another.

### Transaction validation

Canonical transaction payloads, identifiers, supported methods, argument interpretation, size and fee rules, sender encoding, Ed25519 signatures, nonce rules, balances, rewards, and deterministic execution are protected.

Mempool admission is node-local policy, not final consensus acceptance. Every transaction included in a block is validated and executed again through block acceptance.

### Chain selection and cumulative work

Vision-Core selects among valid competing branches using cumulative work rather than height alone. Protected behavior includes:

- target-to-work conversion;
- per-block and cumulative-work arithmetic;
- ancestor discovery;
- branch comparison;
- deterministic tie behavior;
- reorganization construction and application;
- state restoration during disconnect and reconnect.

Advertised peer work is a synchronization hint. It cannot become authoritative without downloading and validating the corresponding blocks.

### State transitions

Transaction application, coinbase crediting, fee handling, nonce progression, UTXO or account-state changes, block connection, block disconnection, and side-chain replay must be deterministic and atomic at the validation boundary.

Rejection must not partially mutate canonical state.

### Validation ordering

Validation order is protected where one check depends on another or where reordering can change acceptance or partially mutate state. Error wording is normally outside consensus, but error categories become protocol-sensitive when callers use them to retry, disconnect peers, pause mining, select branches, or change recovery behavior.

### Consensus constants

Constants governing block validity, economics, difficulty, proof parameters, versions, or historical activation behavior are consensus-sensitive even when currently dormant. A compiler warning does not make such a constant safe to delete.

Behavioral consensus changes require:

- explicit owner authorization before implementation;
- an isolated design and commit series;
- exact compatibility and activation reasoning;
- deterministic vectors;
- full release validation and all applicable expanded suites.

If an activation mechanism is not already supported by repository evidence, its design is **Owner Decision Required**.

## Proof of Work

Proof-of-work behavior determines whether a block represents acceptable work. Protected areas include:

- historical preimage construction;
- target encoding and conversion;
- difficulty calculation and adjustment;
- hash-to-target comparison;
- mining job inputs;
- nonce handling;
- local and remote block verification;
- cumulative-work derivation;
- historical height or version routing.

Vision-Core preserves previously validated proof semantics. A new implementation may be clearer or faster but cannot replace historical behavior merely because it appears mathematically equivalent.

Every locally mined block enters ordinary block acceptance. Prior mining or cache work does not authorize acceptance to skip proof revalidation.

Proof-of-work changes require exact historical vectors, boundary arithmetic, invalid-proof cases, full release tests, VisionX tests, and applicable multi-node and historical-chain validation.

## VisionX

VisionX is consensus-critical.

Protected behavior includes:

- proof input and historical VPoW encoding;
- dataset and seed derivation;
- dataset size and epoch behavior;
- scratch-space and iteration parameters;
- reads, writes, and hash operations;
- cache keys and values;
- mining and verification interfaces;
- the handshake fingerprint used to reject incompatible peers.

Historical compatibility is intentionally preserved. Compatibility code and vectors must not be removed as dead code without a separate consensus decision.

Any optimization, cache change, concurrency change, cleanup, or implementation substitution must demonstrate byte-for-byte and result-for-result equivalence for the history and inputs it governs. A cache hit and a clean recomputation must produce identical results independent of process history, thread scheduling, or platform.

Changes to VisionX behavior require explicit owner authorization, isolated commits, exact vectors, focused VisionX validation, full release validation, and protocol compatibility review.

## Genesis

Genesis establishes blockchain identity. Protected inputs include:

- genesis configuration and initial state;
- chain identifier;
- network identifier;
- genesis block and genesis hash;
- economics commitments;
- proof-of-work identity associated with the network.

These values participate in startup or P2P compatibility checks and cannot be casually modified. A node using a different genesis, chain, economics, or proof identity may belong to a different network even if the software version is otherwise identical.

The repository supports checking these identities, but this document does not infer a general genesis-migration or network-launch procedure. Modification, migration, coexistence, and activation rules are **Owner Decision Required**.

Genesis changes require explicit owner authorization, a new-network or migration design, exact identity vectors, persistence isolation, handshake compatibility tests, full release validation, and release documentation.

## Serialization

Serialization is consensus-sensitive whenever bytes participate in hashing, identifiers, signatures, persistence validation, or network compatibility.

Protected representations include:

- canonical block and block-header encoding;
- canonical transaction payload and transaction identifier encoding;
- field ordering;
- integer widths and signedness;
- byte order;
- collection ordering;
- transaction-root inputs;
- state-root inputs;
- historical VPoW preimages;
- binary persisted block shapes;
- P2P message framing and encoding.

Semantically similar values with different bytes are not consensus-equivalent. Serialization must be deterministic across platforms and independent of map iteration, locale, scheduler order, or process state.

Network encoding can be protocol-sensitive without changing block validity. Persistence encoding can be storage-sensitive without changing new-block rules. Both still require compatibility review because incompatible nodes or unreadable databases are release failures.

Any serialization change requires explicit owner authorization, exact before-and-after byte analysis, golden vectors, compatibility tests, and all validation applicable to the consumers of those bytes.

## State Root

The state root is a deterministic commitment to consensus-relevant state at a defined block. It allows nodes to prove that transaction execution produced the state committed by the block.

Protected behavior includes:

- the set of state included;
- account, UTXO, balance, and nonce encoding as applicable;
- entry ordering;
- hashing and tree construction;
- block-connection and side-chain state calculation;
- snapshot commitment verification;
- restart reconstruction;
- reorganization state restoration.

A state-root defect can cause a valid block to be rejected, an invalid state transition to be accepted, or nodes to disagree after restart.

State-root changes require explicit owner authorization, deterministic vectors, full release tests, ChainState tests, reorganization tests, snapshot tests, persistence/restart tests, and historical compatibility analysis.

## Persistence

Vision-Core uses sled to store blocks, canonical height indexes, tip metadata, snapshot metadata, state-root commitments, and snapshot state. `ChainState` itself is reconstructed rather than serialized as one struct.

Protected persistence behavior includes:

- database key layout;
- serialized block and metadata shapes;
- canonical height-to-hash indexes;
- tip identity;
- cumulative-work reconstruction;
- snapshot lineage and state-root verification;
- startup replay order;
- block connection and reorganization writes;
- corruption and incompatibility handling.

Restart must reconstruct the same selected tip and committed state from the same durable history. Snapshots may accelerate recovery but cannot override canonical lineage or bypass state-root verification.

Database compatibility must be maintained unless an intentional migration is documented, implemented, and validated. The repository does not currently establish a general database schema-versioning or migration framework. The long-term migration and rollback policy is **Owner Decision Required**.

Persistence-format changes require explicit owner authorization, isolated commits, old-database fixtures or equivalent compatibility evidence, storage tests, restart tests, reorganization tests, snapshot/state-root tests, full release validation, and operator migration documentation.

## Networking

Networking supplies untrusted candidate history to consensus. It can indirectly affect consensus even when block-validity rules do not change.

Protected networking behavior includes:

- peer protocol and version compatibility;
- chain, network, genesis, economics, and VisionX handshake identity;
- P2P framing and message bounds;
- peer discovery and advertised identity;
- chain-summary handling;
- synchronization peer selection;
- missing-block download and import;
- watchdog stall detection and recovery;
- prevention of overlapping synchronization;
- higher-work recovery and mining pause coordination;
- deterministic test scenarios.

A networking change can prevent a node from discovering a superior valid chain, trust advertised work without validation, import blocks through an incomplete path, or behave differently based on peer arrival or thread scheduling. Those outcomes can leave compatible nodes on inconsistent tips even when the underlying validity function is unchanged.

Synchronization must remain deterministic at its state-transition boundaries. Tests may control peer order and timing to demonstrate recovery; production consensus results must not depend on them.

Protocol-version or compatibility changes require explicit owner authorization. Ordinary internal networking fixes still require focused P2P and watchdog validation plus full release and VisionX suites when the synchronization path can reach block acceptance.

## Configuration

Configuration controls runtime behavior rather than defining consensus. Current settings select network identity, data location, listeners, peers, API behavior, mining options, and logging.

Configuration remains safety-sensitive because an invalid or silently defaulted value can start a node on the wrong interface, use an unintended data directory, disable or enable mining, or select incompatible network behavior.

Configuration hardening is permitted only through isolated, explicitly scoped commits with startup validation. It intentionally changes observable behavior by replacing some silent fallback with actionable failure.

Configuration work must not:

- redefine consensus constants through operator input;
- permit persisted data from one network to be silently used by another;
- change genesis, chain, or VisionX identity without protocol authorization;
- conceal behavior changes as parsing cleanup.

The promoted P2P hardening work preserves the established permissive
private-peer default while rejecting invalid explicit values. The exact
disposition of `VISION_CONFIG`, `VISION_MINING_THREADS`, mining policy, and
operator migration remains governed by separately approved Configuration
Hardening work. Unsupported configuration-file precedence or migration
semantics are **Owner Decision Required** until designed.

## Developer Readiness Boundaries

Developer-readiness work includes:

- documentation;
- formatting;
- warning measurement and cleanup;
- dead-code classification;
- test-harness maintenance;
- logging consistency;
- error-message improvement;
- developer tooling.

These concerns must never be combined with consensus-sensitive modifications.

Formatting belongs in an isolated commit. Documentation should precede cleanup so ownership and compatibility are known. Warning cleanup requires classification. Dead code in historical proof, dormant protocol, public façade, persistence, or state-model areas remains protected until separately authorized.

Passing a test suite does not broaden the approved scope. A cleanup that unexpectedly reaches canonical encoding, proof behavior, state transition, persistence, or network compatibility stops and is reclassified.

## Validation Requirements

[TESTING_POLICY.md](TESTING_POLICY.md) is the single authority for minimum
validation by change class. Its matrix covers documentation, formatting,
warning and dead-code cleanup, configuration, networking, persistence,
consensus, VisionX, protocol compatibility, and release candidates.

Classify a change here first, then apply the highest-risk matching row in the
Testing Policy. The subsystem sections above explain why expanded evidence is
required; they do not create a second validation matrix.

## Owner Authorization

Explicit repository-owner approval is required before implementation for:

- consensus behavior changes;
- protocol-version changes;
- genesis modifications;
- VisionX behavior changes;
- proof-of-work changes;
- canonical or compatibility serialization changes;
- persistence-format changes;
- state-root algorithm or input-set changes;
- network compatibility changes;
- consensus or economics constant changes;
- historical compatibility removal;
- database migration or rollback policy;
- Release Promotion;
- annotated tag creation and publication;
- force pushes;
- repository history rewrites;
- public tag movement or recreation, which is prohibited under current release policy;
- deletion of archival or release-governance refs.

Configuration Hardening has task-level approval only within its documented scope and gating. It does not authorize consensus, genesis, protocol, persistence-format, or network-identity changes.

Routine documentation, formatting, and already classified low-risk cleanup may proceed only when separately placed in scope. They never inherit authority to modify protected components.

## Future Guidance

When uncertainty exists about whether a change affects consensus, treat it as consensus-sensitive until evidence proves otherwise.

The engineer should:

1. stop implementation at the uncertain boundary;
2. inspect source, tests, vectors, history, release evidence, and persistence behavior;
3. describe the possible block, state, storage, or compatibility effect;
4. identify missing evidence as **Owner Decision Required**;
5. request the safer authorization and validation path;
6. isolate the eventual work from cleanup and unrelated behavior.

Vision-Core should choose the safer review path over the faster one. The cost of additional review is bounded; the cost of an accidental consensus split or historical-data incompatibility is not.

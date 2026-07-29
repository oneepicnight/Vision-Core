# Glossary

This glossary defines Vision-Core terms as used in the repository. It is not a substitute for the implementation or consensus policy.

Named projects and initiatives use title case: Vision-Core, Vision Desktop,
VisionX, Developer Readiness, Configuration Hardening, Developer Foundation,
Dead Code Classification, Dead Code Cleanup, and Release Promotion. Generic
technical concepts use sentence case in prose: consensus, watchdog, state root,
snapshot, and persistence. `ChainState` retains its Rust type spelling.

## A

**Authoritative branch**
The branch designated by repository governance as the current public source of truth, normally `main`. Authority is established by policy and verified refs, not by a local branch name.

## B

**Block header**
The consensus-sensitive block metadata whose canonical encoding participates in block identification and proof-of-work validation.

**Block root**
A generic term for a commitment associated with a block. Use the precise implemented term—such as Merkle root or state root—because the inputs and semantics differ.

## C

**Canonical encoding**
The one byte representation used for consensus hashing, signing, identifiers, or validation. Field order, width, and byte order are part of the rule.

**Chain state**
The in-memory and persisted data required to validate and extend the selected chain, including the tip, UTXO state, cumulative work, and commitment metadata.

**Consensus**
The deterministic rules by which nodes decide whether blocks and transactions are valid and which valid chain is authoritative.

**Consensus boundary**
The set of code and data whose behavior can change block validity, chain selection, or committed state.

**Configuration Hardening**
The approved behavior-changing initiative to replace selected silent
configuration fallback with explicit startup validation and operator guidance.

**Cumulative work**
The total proof-of-work represented by a chain. Vision-Core uses cumulative work, not height alone, to compare competing valid branches.

## D

**Determinism**
The property that equivalent inputs produce equivalent protocol results independent of platform, scheduling, network arrival order, or process history.

**Dead Code Classification**
The completed Developer Readiness tranche that classified unused-code findings
by ownership, risk, and removal prerequisites without authorizing deletion.

**Dead Code Cleanup**
The completed Developer Readiness tranche that implemented only the approved
Tranche 3A and 3B dispositions.

**Developer Foundation**
The completed Developer Readiness tranche that established developer
documentation, toolchain identity, release identity, source-documentation
repair, and baseline CI.

**Developer Readiness**
The post-v1.0.4 initiative that moved Vision-Core from a validated codebase to
a documented, classified, reviewable engineering project.

## F

**Fork choice**
The rule used to select among competing valid chains. In Vision-Core, the central comparison is cumulative work.

## H

**Historical compatibility**
The requirement that current software preserve the interpretation and validation of already-established chain history, including earlier encodings where applicable.

## M

**Mempool**
The node-local collection of valid transactions not yet included in the selected chain. Mempool policy is normally non-consensus, but its interaction with persistence and state must still be audited.

**Merkle root**
A deterministic commitment to an ordered collection, typically transactions. Its construction and leaf encoding are consensus-sensitive when included in a block header.

## N

**Network identifier**
The value used to distinguish Vision-Core networks during peer communication and configuration. It is protocol-sensitive.

**Node harness**
Test infrastructure that starts and controls nodes for integration or network tests. Harness behavior is not production consensus, but nondeterminism can invalidate test evidence.

## P

**P2P**
Peer-to-peer networking through which nodes discover peers and exchange chain and transaction data without a central relay requirement.

**Persistence**
The durable representation of chain state and metadata used across process restarts.

**Protocol compatibility**
The ability of independently running nodes or clients to communicate and interpret messages consistently. A protocol break may occur without changing block consensus.

## R

**Reorganization (reorg)**
Replacement of part of the selected chain with a competing valid branch having superior fork-choice weight. Correct reorganization restores and reapplies state deterministically.

**Release identity**
The immutable association among an annotated release tag, its commit, version metadata, and published release notes.

**Release Promotion**
The explicitly authorized advancement of a validated candidate to the
authoritative branch and, under separate authorization, its annotated release
tag and publication.

## S

**Snapshot**
A portable representation of validated chain state used to accelerate initialization or recovery. Snapshot metadata and state roots must be verified before trust.

**State root**
A deterministic commitment to the consensus-relevant state at a defined point. Its input set, ordering, and encoding are consensus-sensitive.

## T

**Target**
The proof-of-work threshold against which a derived hash value is compared. Target encoding and arithmetic are consensus-sensitive.

**Test oracle**
The expected result against which a test compares behavior. Consensus tests should use explicit, stable oracles such as known encodings, hashes, roots, and state transitions.

## U

**UTXO**
Unspent transaction output. UTXOs represent spendable outputs and form a central part of Vision-Core transaction validation and chain state.

## V

**Vision**
The broader project mission: a decentralized protocol foundation for durable digital ownership, exchange, identity, creator economies, gaming, and persistent virtual worlds.

**Vision-Core**
The core blockchain node and protocol implementation in this repository.

**VisionX**
Vision-Core’s proof-of-work subsystem. Its encoding, dataset/cache derivation, historical compatibility, and validation behavior are consensus-sensitive.

**VPoW**
The versioned or historical proof-of-work encoding path preserved for compatibility with established Vision-Core chain history. Do not normalize or replace it without a consensus decision.

## W

**Watchdog**
The P2P synchronization recovery mechanism that detects lack of progress and enables recovery or peer replacement. Its deterministic regression test controls test peer ordering without changing production peer behavior.

# Roadmap

## How to Read This Roadmap

This roadmap separates completed work, currently approved work, planned engineering, and visionary goals. It is not a delivery schedule and does not claim that planned systems already exist.

Protocol foundations precede applications. Any item that changes consensus, persistence, protocol compatibility, or operator behavior requires its own design, authorization, validation, and release plan.

## Completed

Status: completed and promoted to the authoritative public branch. Current
documentation on `main` is maintained as a living record of that promoted
state.

- preserved historical proof-of-work and VisionX compatibility;
- established cumulative-work fork choice and state-aware reorganization;
- hardened snapshots, restart reconstruction, and persistence;
- released `vision-core-consensus-v1.0.4` as an immutable authoritative baseline;
- made the watchdog recovery regression deterministic;
- completed the Developer Foundation documentation and governance stack;
- aligned package and runtime release identity;
- pinned the validated Rust toolchain;
- documented architecture, API, configuration, contribution, security, and consensus policy;
- completed the Formatting and Warning Baseline tranche;
- established baseline CI and a clean formatting baseline;
- completed the Dead-Code Classification tranche;
- classified warning and dead-code debt;
- completed owner-approved low-risk and ChainState cleanup tranches;
- activated Configuration Hardening and promoted Tranches 1 through 4B,
  including the typed settings seam, Tokio runtime-thread validation, and
  explicit `VISION_DATA_DIR` startup validation;
- created the Project Intelligence Layer and initial decision records.

Completion here describes engineering work and recorded validation. Public authority changes only after review and promotion.

## Current

### Configuration Hardening

Status: active engineering phase. Tranches 1 through 4B are promoted; no later
tranche is authorized yet.

Configuration Hardening is the current operator-behavior hardening program.
Promoted work has already added a typed settings seam, strict
`TOKIO_WORKER_THREADS` validation, and explicit `VISION_DATA_DIR` validation
before storage opens.

Remaining approved direction includes replacing other silent fallback with
explicit startup validation, reconciling `VISION_CONFIG`, resolving
`VISION_MINING_THREADS`, validating mining configuration, clarifying
private-peer policy, and documenting operator migration. Each later behavior
change still requires its own authorization, validation, review branch,
pull-request CI, and promotion gate.

### Repository polish

Status: ongoing maintenance alongside Configuration Hardening.

- keep current-state and roadmap documentation synchronized with promoted work;
- keep warning and dead-code classifications current;
- preserve one-concern-per-commit history;
- avoid any new cleanup beyond explicit authorization;
- decide repository licensing, security intake, Rust API policy, and GitHub Action pinning.

## Near Term

Status: planned; individual tasks require approval.

### Vision-Core v1.1.0

Vision-Core v1.1.0 is the intended milestone for the first release developed
entirely under the documented engineering operating system. Its exact feature
scope, compatibility classification, and release candidate remain **Owner
Decision Required**. The version must not be tagged or promoted until the
Developer Readiness stack and Configuration Hardening work have completed
their applicable review and validation.

### API cleanup

- centralize HTTP error envelopes;
- define stable error codes and status semantics;
- connect `/peers` to live state or remove the unsupported claim;
- define API versioning and compatibility;
- publish OpenAPI or an equivalent machine-readable contract.

### Logging improvements

- standardize structured tracing fields;
- align log levels and subsystem naming;
- improve startup, recovery, synchronization, mining, and persistence diagnostics;
- avoid leaking secrets or producing nondeterministic test assertions.

### Configuration validation

- complete the approved hardening tranche;
- add exact valid/invalid configuration cases;
- document migration and operator-facing failures;
- ensure configuration changes do not silently alter network or consensus identity.

### Documentation completion

- maintain current-state and decision records with each promoted change;
- add operator deployment, backup, restore, corruption, and upgrade runbooks;
- define security and API support policies;
- document supported platforms and network profiles.

### Developer tooling

- provide repeatable local validation commands;
- improve test selection and evidence capture;
- add contract-generation or schema checks where appropriate;
- decide Clippy ratcheting without hiding classified debt;
- define dependency and GitHub Action update policy.

## Medium Term

Status: planned architecture; designs and interfaces are not yet approved protocol behavior.

### Exchange infrastructure

- design peer-to-peer exchange discovery and settlement boundaries;
- keep indexes and order discovery outside consensus authority;
- define atomicity, fees, failure recovery, and compatibility.

### Wallet integration

- define supported signing and transaction-construction contracts;
- provide balance, nonce, history, and submission flows;
- establish key-management and recovery boundaries.

### Desktop integration

- update Vision Desktop to a reviewed Vision-Core baseline;
- improve verified Core installation and process lifecycle;
- expose node health, synchronization, mining, configuration, and diagnostics;
- integrate wallet and future service interfaces without duplicating consensus.

### Marketplace foundations

- specify asset identity, provenance, listings, and creator-payment requirements;
- distinguish derived search/index data from on-chain ownership;
- define enforcement and trust boundaries.

### SDK development

- create supported client libraries for API and transaction contracts;
- publish deterministic serialization and signing vectors;
- establish compatibility and versioning policy.

### Node usability

- add observable health and readiness;
- improve peer and synchronization visibility;
- produce safe backup, restore, and upgrade flows;
- define deployment profiles and resource guidance.

## Long Term

Status: planned protocol and ecosystem direction. Consensus-affecting items require formal activation design.

### Decentralized exchange

- non-custodial asset settlement;
- resilient peer-to-peer discovery;
- explicit liquidity, fee, and dispute boundaries;
- scalable settlement mechanisms.

### Land ownership

- durable virtual-land identity;
- issuance, transfer, subdivision, and provenance;
- application enforcement and governance rules;
- clear distinction between protocol ownership and product presentation.

### Staking systems

- determine whether staking has a protocol, service, governance, or application role;
- specify economics and security before implementation;
- avoid implying that staking exists in the current proof-of-work protocol.

### Governance mechanisms

- define proposal, activation, voting, and emergency-change authority;
- preserve deterministic node behavior and historical auditability;
- document the relationship between social governance and protocol enforcement.

### Creator economy support

- programmable creator payments and royalties where enforceability is explicit;
- durable attribution and provenance;
- direct creator-to-user exchange;
- transparent fee and treasury policy.

### Scalable networking

- improve peer discovery and persistence;
- strengthen resistance to malicious or low-quality peers;
- scale synchronization, propagation, and recovery;
- preserve the rule that advertised state is never authority.

## Future Vision

Status: visionary goal, not an implementation commitment.

Vision’s long-term objective is to become the blockchain foundation for persistent online worlds in which digital ownership exists independently of any single game, publisher, marketplace, cloud provider, or company.

In that future:

- people retain assets and identity beyond the lifecycle of one application;
- creators participate directly in the economies they help build;
- virtual land and goods have durable provenance;
- wallets, exchanges, marketplaces, and games interoperate through open protocol services;
- peer-to-peer exchange reduces dependence on custodial intermediaries;
- online worlds can evolve while their ownership history remains independently verifiable.

Gaming remains the flagship expression of the vision, not the protocol itself. Rendering, simulation, and cloud execution may occur in application layers. Vision-Core supplies the durable ownership, settlement, and verification foundation beneath them.

An item moves from vision to planned work only after its ownership, security model, protocol boundary, and compatibility requirements are documented. It becomes implemented only after source is promoted, required validation passes, and [CURRENT_STATUS.md](CURRENT_STATUS.md) records the result.

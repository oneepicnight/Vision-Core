# Current Status

## Status Date

2026-07-30, America/New_York.

This file distinguishes the immutable v1.0.4 release tag from the current
authoritative `main` development baseline. They must not be conflated.

## Authoritative Release

- Release: Vision-Core Consensus v1.0.4
- Tag: `vision-core-consensus-v1.0.4`
- Commit: `b874d73cbdf60657334b62c867ed7f18b80a186b`
- Tree: `d650bb3419db56cce9e1d789611763e5cb4cbc26`
- Authoritative public branch: `origin/main`

The only source delta from v1.0.3 to v1.0.4 is a deterministic correction to P2P watchdog recovery test infrastructure. It does not change runtime consensus, networking behavior, proof of work, VisionX, mining, persistence, API behavior, or protocol compatibility.

Historical tags remain immutable. `vision-core-consensus-v1.0.3` and `vision-core-alpha-rc2` continue to identify `6a065df8206b50874029a27ee2b54dffae5e3cdd`. The prior public main state is preserved at `archive/main-pre-v103-032a0f2`.

## Current Development Line

- Long-lived integration branch: `dev/configuration-hardening-v104`
- Current promoted code baseline:
  `b23ca0c53706c095acb0dd48b5ab5593166ac8ab`
- Promoted code tree: `0bb6f9854972dab20babe5b4bccd67b6a24dbebd`
- Current `origin/main`: `b23ca0c53706c095acb0dd48b5ab5593166ac8ab`

Configuration Hardening review uses short-lived per-tranche branches. Tranche
4B was reviewed through pull request #7 and promoted to `main` by normal
fast-forward. The long-lived local and remote integration branches are
synchronized with that promoted commit.

## Current Validation Baseline

The latest completed Configuration Hardening Tranche 4B evidence records:

- tests discovered: 535;
- passed: 534;
- failed: 0;
- ignored: 1;
- focused watchdog: 1 passed, 0 failed;
- VisionX module: 32 passed, 0 failed;
- focused data-directory tests: 11 passed, 0 failed;
- storage tests: 9 passed, 0 failed;
- bootstrap/restart tests: 14 passed, 0 failed, 1 ignored;
- reorganization tests: 16 passed, 0 failed;
- snapshot tests: 17 passed, 0 failed;
- state-root tests: 10 passed, 0 failed;
- formatting: clean.

The sole ignored test is `node::bootstrap::tests::bootstrap_recovery_worker`.

These totals describe the recorded developer-line validation. They are not copied forward as proof for a later commit; any changed candidate requires fresh validation.

## Warning Baseline

- Tranche 2 baseline: 58 normal-target warnings and 34 test-target warnings.
- Current post-Tranche 4B baseline: 58 normal-target warnings and 30 test-target warnings.
- Formatting baseline: clean.
- Clippy: intentionally non-blocking while classified debt remains.

The three-warning reduction in the test target occurred during the approved
Tranche 3A/3B cleanup. Those tranches removed genuinely unused
test-infrastructure and in-memory state-model code and restored the omitted
single-block cumulative-work test to the discovered test inventory. The normal
binary warning count remained 58 because the removed findings were specific to
test or test-expanded compilation. This is a measured baseline transition, not
an unexplained recount or authorization for further warning removal.

The later reduction from 31 to 30 test-target warnings was first recorded by
the Configuration Hardening characterization baseline. Its deterministic
settings/runtime tests made one previously dead test-expanded path reachable;
no lint suppression or production-code removal produced that reduction.

Warnings have been classified. The totals are not blanket authorization to delete code or suppress diagnostics.

## Completed Engineering Tranches

### Developer Foundation

- developer-facing README and engineering documentation;
- architecture, API, configuration, contribution, security, and consensus policy;
- exact Rust 1.97.1 toolchain declaration;
- package and runtime identity aligned with v1.0.4;
- corrupted source-documentation encoding repaired;
- baseline build and release-test CI.

### Formatting and Warning Baseline

- isolated repository-wide Rust formatting;
- removal of unambiguous unused imports and test mutability;
- updated temporary-directory handling;
- trivial test-only Clippy cleanup;
- documented compiler and Clippy baseline;
- blocking formatting check.

### Dead Code Classification

- 48 ledger entries classified by ownership and risk;
- historical compatibility and consensus-sensitive items protected;
- public, planned, and uncertain items frozen;
- ChainState mempool fields referred to a separate persistence/state-model audit.

### Dead Code Cleanup

- obsolete bootstrap test helper removed;
- unused `NodeHarness::api_addr` removed;
- `#[test]` restored on `cumulative_work_single_block`;
- four unread, non-persisted ChainState mempool fields removed in one isolated commit;
- focused, release, VisionX, persistence, restart, reorganization, snapshot, and state-root validation completed.

### Configuration Hardening

- current configuration and runtime-thread behavior characterized before
  behavior changes;
- source-neutral typed settings translation seam introduced without changing
  configuration behavior;
- invalid `TOKIO_WORKER_THREADS` values now fail before Tokio runtime
  construction with an actionable structured error;
- missing runtime-thread configuration retains logical-CPU selection;
- positive integer runtime-thread configuration remains accepted;
- Tranche 3 commit `cbaf619b5420ee90c4b8dedb208699566cf0e182`
  passed pull-request CI and post-promotion `main` CI before the short-lived
  review branch was retired.
- Tranche 4A commit `8089c046bc7193ac1863b81e7502c0808769b7a3`
  characterized existing `VISION_DATA_DIR` behavior and established the
  approved persistence-sensitive policy without changing runtime behavior;
- Tranche 4B commit `b23ca0c53706c095acb0dd48b5ab5593166ac8ab`
  validates the effective data directory before storage initialization,
  rejects explicitly invalid or unusable locations without fallback, reports
  the effective location, and preserves valid paths and the existing
  `chain.db` layout;
- Tranche 4B passed pull-request CI run `30588116415` and post-promotion
  `main` CI run `30590200588` before its short-lived review branch was retired.

## Remaining Technical Debt

- invalid scalar configuration values can still silently fall back to defaults;
- `VISION_CONFIG` is documented but not implemented;
- `VISION_MINING_THREADS` is parsed but not consumed;
- private-peer policy defaults permissive;
- HTTP error envelopes and status semantics are inconsistent;
- `/peers` remains a stub;
- no OpenAPI or declared API versioning policy exists;
- the Core router has no built-in authentication or TLS;
- peer persistence ownership is unresolved;
- transaction-gossip deduplication ownership is unresolved;
- mining statistics and threading interfaces remain unsettled;
- one bootstrap/recovery test remains ignored;
- compiler and Clippy warning debt remains classified but unresolved;
- node deployment, backup, recovery, upgrade, and monitoring runbooks are incomplete.

## Frozen Ledger Items

The remaining [DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md) entries are intentionally frozen. No removal is currently authorized for:

- dormant consensus or protocol features;
- historical VisionX or VPoW compatibility;
- public façades pending a Rust library API decision;
- near-term planned APIs;
- test support that characterizes security or consensus behavior;
- uncertain ownership items;
- persistence or state-model items without dedicated audit evidence;
- historical economics or compatibility constants.

Frozen means preserved pending a design or owner decision. It does not mean the item is permanently required or already approved for implementation.

## Unresolved Owner Decisions

This is the central register of unresolved owner decisions. Detailed context
remains in the linked policy or ledger.

| ID | Owner Decision Required | Controlling context |
| --- | --- | --- |
| OD-01 | Select an authoritative repository license and effective scope. | [LICENSE_DECISION_REQUIRED.md](LICENSE_DECISION_REQUIRED.md) |
| OD-02 | Select and maintain an authoritative private security-reporting channel. | [SECURITY.md](../SECURITY.md) |
| OD-03 | Decide whether Vision-Core exposes a supported third-party Rust library API. | [DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md) |
| OD-04 | Select GitHub Action pinning, dependency-update cadence, and supply-chain review policy. | [ROADMAP.md](ROADMAP.md) |
| OD-05 | Authorize any additional dead-code removal and resolve the ownership questions grouped in the frozen ledger. | [DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md) |
| OD-06 | Define consensus or protocol activation and upgrade governance before any redesign. | [CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md) |
| OD-07 | Define genesis modification, new-network, coexistence, and migration policy. | [CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md) |
| OD-08 | Define database schema versioning, migration, downgrade, and rollback policy. | [CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md) |
| OD-09 | Resolve configuration-file precedence and operator migration details within Configuration Hardening. | [CONFIGURATION.md](CONFIGURATION.md) |
| OD-10 | Choose the recovery method for a failed candidate after authoritative-branch promotion. | [RELEASE_PROCESS.md](RELEASE_PROCESS.md) |
| OD-11 | Define emergency chain and database incident-recovery authority and procedures. | [ENGINEERING_PLAYBOOK.md](ENGINEERING_PLAYBOOK.md) |
| OD-12 | Approve the exact scope and compatibility classification of the planned Vision-Core v1.1.0 milestone. | [ROADMAP.md](ROADMAP.md) |

## Current Work: Configuration Hardening

Configuration Hardening is active on `dev/configuration-hardening-v104`.
Tranches 1 through 4B are promoted. No later tranche is authorized by this
status record; the next scope requires a roadmap review and explicit owner
authorization.

Its intended scope includes:

- replacing silent parsing fallback with typed, actionable validation;
- reconciling the undocumented or unimplemented `VISION_CONFIG` behavior;
- resolving `VISION_MINING_THREADS`;
- validating miner identity and addresses at startup;
- making private-peer policy explicit;
- documenting accepted values and operator migration.

Later work may intentionally change runtime startup behavior for other invalid
settings that still fall back. Each behavior change requires its own
authorization, isolated commit, applicable focused and full validation,
short-lived review branch, pull-request CI, promotion gate, and documentation
closeout.

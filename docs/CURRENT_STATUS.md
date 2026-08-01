# Current Status

## Status Date

2026-08-01, America/New_York.

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

- Long-lived integration branch: `dev/p2p-session-stability-v104`
- Current promoted code baseline:
  `8686bbd44689538e53020e4a3d547d57f73949be`
- Promoted code tree: `e98e831239d8d350448566c79e0d07a8fbfad25a`
- Current `origin/main`: `a15509ddbd8aefcc748f6774016f74f8424d5445`
- Current `origin/main` tree: `a4324e500ef80900dcb5fd770b7b58391d99006a`
- Current local and remote integration baseline:
  `a15509ddbd8aefcc748f6774016f74f8424d5445`

Configuration Hardening review uses short-lived per-tranche branches. Tranche
4B was reviewed through pull request #7 and promoted to `main` by normal
fast-forward. A separate documentation-only closeout commit then advanced
`main` to `c9fad4626eabb352b3f54f6a82536f5a3c7f4067` without changing source,
tests, Cargo, dependencies, or CI. The later documentation synchronization
commit `52e1aae53c6a135718f76ada48d1524d3e33a6f5`, P2P configuration hardening
commit `9a2099273127d6c8135bada8b8bab47c9190c25e`, and service startup
sequencing commit `da72260dcb9cb66971c433e05a6633a4800dfe4d` were subsequently promoted
through independent review and CI cycles. Readiness policy and design,
internet-soak characterization, block dissemination, peer discovery, session
ownership, and mining recovery were then promoted through the same isolated
review process. The documentation-only validation-report commit
`a15509ddbd8aefcc748f6774016f74f8424d5445` records the successful local
three-node rehearsal without changing runtime code. The long-lived local and
remote integration branches are synchronized with the current `main` tip.

## Current Validation Baseline

The latest published numeric release-suite baseline in the P2P internet-soak
characterization records 564 passed, 0 failed, and 1 ignored. The later
session-ownership commit `3e2b3bd8bee1df2f2d109b8bddef5ae6b1c35fc9`,
mining-recovery commit `8686bbd44689538e53020e4a3d547d57f73949be`,
and documentation report commit `a15509ddbd8aefcc748f6774016f74f8424d5445`
each passed the blocking pull-request and post-promotion release-suite jobs.
Those CI summaries did not publish a replacement numeric test count, so this
document does not infer one.

The completed three-node validation at `8686bbd` additionally records:

- three independently persisted mining nodes formed a direct mesh;
- all three miners contributed blocks and converged;
- B and C retained direct operation after seed node A stopped;
- a controlled fork resolved by cumulative work;
- all three nodes resumed mining and reconverged after persistence restart;
- fatal errors, panics, and unexpected process exits: 0;
- final cleanup found no remaining node processes or occupied node ports.

The exact results and evidence boundaries are recorded in
[THREE_NODE_MINING_RELAY_RECOVERY_VALIDATION_REPORT.md](THREE_NODE_MINING_RELAY_RECOVERY_VALIDATION_REPORT.md).

The earlier Tranche 4B persistence-sensitive evidence remains relevant to the
unchanged storage boundary: 11 focused data-directory tests, 9 storage tests,
14 bootstrap/restart tests with 1 ignored, 16 reorganization tests, 17
snapshot tests, and 10 state-root tests passed at the Tranche 4B candidate.
Those historical counts are not presented as a rerun at the current tip.

The sole ignored test is `node::bootstrap::tests::bootstrap_recovery_worker`.

These totals describe the recorded developer-line validation. They are not copied forward as proof for a later commit; any changed candidate requires fresh validation.

## Warning Baseline

- Tranche 2 baseline: 58 normal-target warnings and 34 test-target warnings.
- Current P2P hardening baseline: 57 normal-target warnings and 29 test-target warnings.
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

The later P2P hardening work reduced the baseline from 58/30 to 57/29 by
activating the previously unused outbound-peer target. No lint suppression was
added, and the transition is recorded in
[P2P_INTERNET_SOAK_CHARACTERIZATION.md](P2P_INTERNET_SOAK_CHARACTERIZATION.md).

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
- P2P configuration boundary hardening commit
  `9a2099273127d6c8135bada8b8bab47c9190c25e` preserves the permissive
  private-peer default when omitted, accepts only explicit `true` or `false`,
  distinguishes an explicitly empty seed list from an omitted one, rejects
  malformed non-empty seed input during configuration loading, and requires
  advertised host and port to be supplied together;
- service startup sequencing commit
  `da72260dcb9cb66971c433e05a6633a4800dfe4d` binds the P2P listener before
  detaching its task, treats P2P and API bind failures as startup failures, and
  emits `[NODE] All services started` only after both required listeners bind;
- seed reachability remains outside startup-readiness policy: dialing may fail
  after local startup without redefining chain or listener readiness.
- readiness and health-state policy is approved and documented without yet
  implementing the typed runtime model or HTTP surfaces;
- accepted blocks are relayed through bounded existing P2P messages and stale
  mining jobs are cancelled after peer tip advancement;
- handshake-time peer exchange and supervised dialing can form direct
  non-seed sessions;
- automatic P2P port mode derives a stable local port from the routed IP while
  leaving public-address discovery, firewall, and NAT traversal to operators;
- concurrent peer sessions retain independent ownership so closing one session
  does not tear down another live session for the same peer;
- miners resume after announced recovery once canonical convergence permits;
- the local three-node mining, relay, seed-independence, partition-recovery,
  and full persistence-restart rehearsal passed at `8686bbd`.

## Remaining Technical Debt

- invalid HTTP/P2P ports and several non-P2P scalar settings can still silently
  fall back to defaults or disabled values;
- `VISION_CONFIG` is documented but not implemented;
- `VISION_MINING_THREADS` is parsed but not consumed;
- private-peer policy still defaults permissive when omitted, now as an
  explicit preserved policy rather than an invalid-value fallback;
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

## Current Work: Distributed-Network Validation

Configuration Hardening through service startup sequencing is promoted. The
active integration line is `dev/p2p-session-stability-v104`, synchronized with
`main`. Vision-Core has moved from configuration-boundary work into targeted
distributed-network validation. This status does not classify Vision-Core as
generally production-ready or Internet-soak validated.

The readiness and health-state policy is approved in
[ADR-0009](DECISIONS/0009_readiness_health_state_policy.md) and formalized in
[READINESS_HEALTH_MODEL_DESIGN.md](READINESS_HEALTH_MODEL_DESIGN.md). It defines
role-aware liveness, readiness, degradation, versioned operational surfaces,
and diagnostic retention/redaction. It authorizes design only; no typed
readiness model, readiness route, setting, or runtime behavior has been
implemented by that approval.

The next operational milestone is the documented four-computer, 48-hour
Internet soak: two physical laptops and two virtual machines, with all four
nodes mining and one node serving only as the initial seed. The successful
local three-node rehearsal is a prerequisite, not WAN or endurance evidence.
The soak must independently validate NAT and firewall setup, direct non-seed
peer discovery, block contribution from every miner, convergence, restart
recovery, and bounded resource growth.

Remaining hardening scope includes:

- replacing silent parsing fallback with typed, actionable validation;
- reconciling the undocumented or unimplemented `VISION_CONFIG` behavior;
- resolving `VISION_MINING_THREADS`;
- validating miner identity and addresses at startup;
- retaining the now-explicit private-peer default while improving operator
  diagnostics;
- documenting accepted values and operator migration.

No later implementation tranche is authorized by this status record. New
runtime work requires explicit owner authorization. Each behavior change
requires its own isolated commit, applicable focused and full validation,
short-lived review branch, pull-request CI, promotion gate, and documentation
closeout.

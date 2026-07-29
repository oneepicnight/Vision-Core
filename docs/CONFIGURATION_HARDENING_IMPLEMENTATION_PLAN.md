# Configuration Hardening Implementation Plan

## Purpose

This plan defines a reviewable path for Configuration Hardening after the
Developer Readiness stack is committed, pushed, validated, and promoted.

It does not authorize implementation. It does not choose unresolved owner
decisions. It does not make consensus, protocol, genesis, VisionX,
serialization, or persistence-format values configurable.

The inventory supporting this plan is
[CONFIGURATION_HARDENING_INVENTORY.md](CONFIGURATION_HARDENING_INVENTORY.md).

## Non-Negotiable Boundaries

- Begin from a fresh branch created from the promoted `main`.
- One engineering concern per commit.
- Do not combine formatting with logic.
- Do not combine documentation with behavior.
- Do not combine dependency changes with behavior.
- Add characterization evidence before changing behavior.
- Preserve valid-input behavior unless a phase explicitly changes it.
- Reject invalid input before opening persistent state or starting services
  wherever the architecture permits.
- Never expose consensus constants, genesis identity, protocol versions,
  VisionX parameters, canonical encodings, or state-root rules as operator
  configuration.
- Stop at every Owner Decision Required gate.

## Entry Gate: No Configuration Commit

Before creating the Configuration Hardening branch:

1. review and commit the documentation-only foundation under authorization;
2. push `dev/dead-code-cleanup-v104`;
3. allow GitHub Actions to validate the exact pushed commit;
4. review the complete branch diff and CI results;
5. promote to `main` only under explicit authorization;
6. verify remote `main`;
7. create a new branch from the updated `main`.

This entry gate is administrative and governance work, not a Configuration
Hardening commit.

## Proposed Commit Sequence

### Commit 1 — Characterize current configuration behavior

**Purpose**

Add deterministic tests for current parsers, defaults, invalid-value fallback,
setting consumers, and runtime-thread failure boundaries before changing
behavior. Use pure helper calls or subprocess isolation rather than mutating
global environment in parallel tests.

**Expected files**

- `src/config/settings.rs` test module;
- `src/node/runtime.rs` test module;
- a dedicated configuration integration-test module only if subprocess
  isolation cannot remain local.

**Risk level:** Low.

**Consensus impact:** None. Tests only.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused configuration and runtime tests;
- full release suite because the shared test inventory changes;
- CI.

**Rollback complexity:** Low; revert test-only commit.

**Estimated review difficulty:** Low to moderate. Review must ensure tests
characterize current behavior rather than encode the desired future behavior.

### Commit 2 — Introduce a source-neutral typed settings parser

**Purpose**

Separate raw configuration acquisition from pure parsing and introduce typed,
structured configuration errors. Preserve all valid-input and absent-value
behavior. Do not yet tighten an invalid-value rule.

This seam allows unit tests to provide a map or typed raw-input object without
process-global environment mutation.

**Expected files**

- `src/config/settings.rs`;
- `src/config/mod.rs` only if an error or source type requires a module export;
- focused settings tests.

**Risk level:** Low to medium.

**Consensus impact:** None. Behavior-preserving runtime refactor; startup path
only.

**Validation requirements**

- `cargo check --all-targets --locked`;
- parser equivalence and settings tests;
- focused startup tests;
- full release suite;
- CI.

**Rollback complexity:** Low; no persistent data or protocol change.

**Estimated review difficulty:** Moderate because equivalence must be shown for
every current setting.

### Commit 3 — Validate Tokio runtime thread configuration

**Purpose**

Make `TOKIO_WORKER_THREADS` reject invalid and zero values with an actionable
startup error instead of silently falling back or panicking. Preserve logical
CPU selection when absent.

**Expected files**

- `src/node/runtime.rs`;
- `src/main.rs`;
- focused runtime/startup tests.

**Risk level:** Medium.

**Consensus impact:** None intended. Concurrency is operational, but tests must
guard against scheduler-dependent protocol results.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused missing/valid/invalid/zero runtime tests;
- focused watchdog;
- VisionX suite;
- full release suite;
- CI.

**Rollback complexity:** Low; restore prior fallback/panic behavior.

**Estimated review difficulty:** Moderate.

### Commit 4 — Enforce typed scalar syntax

**Purpose**

Reject invalid HTTP and P2P ports and unrecognized boolean strings. Define a
single boolean grammar for mining, private-peer policy, and alpha-airdrop
enablement. Preserve defaults only for missing values.

This commit must not change the selected default values. Changing
`VISION_ALLOW_PRIVATE_PEERS` from permissive to restrictive is a separate owner
decision and separate commit.

**Expected files**

- `src/config/settings.rs`;
- `src/main.rs` if error propagation changes;
- focused parser and startup tests.

**Risk level:** Medium.

**Consensus impact:** None. Observable startup and operator behavior changes.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused port and boolean tests;
- startup/API/P2P listener tests;
- focused watchdog;
- VisionX suite;
- full release suite;
- CI.

**Rollback complexity:** Low; no data migration.

**Estimated review difficulty:** Moderate because formerly accepted invalid
environments will fail.

### Commit 5 — Validate storage selection before opening state

**Purpose**

Define and enforce data-directory rules before sled is opened. Detect empty or
unusable paths and report the setting and operation. Preserve the existing
`chain.db` layout and all stored encodings.

Whether an empty path is rejected and whether paths are canonicalized are owner
decisions to settle before this commit.

**Expected files**

- `src/config/settings.rs`;
- `src/node/bootstrap.rs`;
- focused settings/startup tests;
- persistence test helpers only where required to exercise failure safely.

**Risk level:** Medium to high.

**Consensus impact:** No rule change. Persistence-sensitive because the setting
selects durable chain state.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused data-path and startup tests;
- storage tests;
- restart tests;
- reorg tests;
- snapshot tests;
- state-root tests;
- full release suite;
- CI.

**Rollback complexity:** Medium. No format migration is allowed, but operator
path acceptance changes may require restoring the old executable or correcting
the environment.

**Estimated review difficulty:** High because path semantics and startup order
must remain cross-platform safe.

### Commit 6 — Validate P2P identity and seed configuration

**Purpose**

Validate seed socket addresses before launching connection tasks, validate
advertised host and port as a coherent pair, and produce actionable errors.
Stage or acknowledge the inbound listener bind before reporting services
started so a bind failure cannot leave the process running without its expected
P2P listener.

The private-peer default, explicit-empty seed-list policy, and whether partial
advertised identity is rejected are Owner Decision Required before this
commit. Do not change protocol version, handshake encoding, or peer validation
rules.

**Expected files**

- `src/config/settings.rs`;
- `src/node/services.rs` or `src/node/bootstrap.rs` only for startup staging;
- P2P configuration and handshake tests.

**Risk level:** Medium to high.

**Consensus impact:** No block-rule change. Network compatibility and topology
are operationally sensitive.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused settings, seed, handshake, and advertised-identity tests;
- focused watchdog;
- synchronization and relevant multi-node tests;
- VisionX suite;
- full release suite;
- CI.

**Rollback complexity:** Medium; operators may need to correct addresses or
restore the earlier executable.

**Estimated review difficulty:** High due to reachability and compatibility
effects.

### Commit 7 — Enforce safe mining configuration

**Purpose**

Reject malformed miner addresses rather than substituting the zero address and
resolve `VISION_MINING_THREADS`.

Before implementation, the owner must decide:

- whether a nonzero explicit miner address is required only when mining is
  enabled or whenever the variable is present;
- whether `VISION_MINING_THREADS` is implemented, renamed, or removed;
- whether the setting controls proof-search workers, Tokio workers, or neither.

Only one selected behavior belongs in this commit. A dependency or broad miner
refactor is not included.

**Expected files**

- `src/config/settings.rs`;
- `src/node/services.rs` and miner modules only if the approved thread behavior
  requires them;
- focused settings and mining tests.

**Risk level:** High.

**Consensus impact:** Validation rules remain unchanged. The miner address
changes coinbase recipient and candidate state root; worker changes affect
proof-search execution.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused miner-address and mining-control tests;
- mining candidate and state-root tests;
- focused watchdog;
- VisionX suite;
- full release suite;
- CI.

**Rollback complexity:** Medium. No stored format changes, but an operator may
need to restore configuration to continue mining.

**Estimated review difficulty:** High.

### Commit 8 — Isolate development-only API enablement validation

**Purpose**

Complete strict validation of `VISION_ALPHA_AIRDROP_ENABLED` and verify that the
development-only route is registered only under the accepted explicit value.
Do not change the endpoint’s state-transition behavior.

**Expected files**

- `src/config/settings.rs`;
- API router or alpha tests only if coverage is missing.

**Risk level:** Medium.

**Consensus impact:** None to protocol rules. Runtime API exposure changes for
invalid inputs.

**Validation requirements**

- `cargo check --all-targets --locked`;
- focused settings, router, and enabled/disabled endpoint tests;
- focused watchdog;
- VisionX suite;
- full release suite;
- CI.

**Rollback complexity:** Low.

**Estimated review difficulty:** Moderate.

### Commit 9 — Resolve configuration-file status

**Purpose**

Execute OD-09 only after the owner selects one path.

**Path A: no configuration file**

- remove the unrealized `VISION_CONFIG` source claim;
- retain environment-only behavior;
- evaluate removal of the unused `toml` dependency in a separate dependency
  commit.

**Path B: implement a configuration file**

- define a versioned schema;
- define file, environment, and default precedence;
- reject unknown or invalid fields;
- define missing-file behavior;
- load and validate everything before opening state or starting services;
- do not support runtime reload in this phase.

**Expected files**

- Path A: source documentation in `src/config/settings.rs`; dependency files in
  a separate commit if authorized.
- Path B: `src/config/settings.rs`, a focused configuration-file module, and
  parser/startup tests.

**Risk level:** Low for removing the claim; high for implementing the loader.

**Consensus impact:** None directly. A file can select persistence, networking,
mining, and API behavior.

**Validation requirements**

- Path A: `cargo check`; focused settings tests; full release suite; CI.
- Path B: `cargo check`; schema and precedence tests; invalid/unknown-field
  tests; startup; storage; restart; networking; watchdog; VisionX; snapshot and
  state-root where data selection is involved; full release suite; CI.

**Rollback complexity:** Low for Path A; high for Path B because operators may
adopt the new file and precedence.

**Estimated review difficulty:** Low for Path A; high for Path B.

### Commit 10 — Publish operator migration documentation

**Purpose**

After the final behavior commit is validated, update operator documentation
with accepted syntax, defaults, precedence, failure messages, migration
examples, and rollback instructions. Update `CURRENT_STATUS.md` only when the
implementation is promoted.

**Expected files**

- `docs/CONFIGURATION.md`;
- `README.md`;
- `docs/CURRENT_STATUS.md` at promotion time;
- a decision record only if an architectural or owner decision warrants one.

**Risk level:** Low.

**Consensus impact:** None. Documentation only.

**Validation requirements**

- Markdown file and anchor checks;
- `git diff --check`;
- documentation-only scope audit;
- applicable documentation CI.

**Rollback complexity:** Low.

**Estimated review difficulty:** Low to moderate; examples must match the
validated implementation exactly.

## Validation Matrix by Proposed Commit

Legend: **Required** means the phase must run the gate. **Conditional** means
run it when the selected design touches that subsystem. **No** means the gate
does not add evidence proportional to the phase’s risk.

| Commit | `cargo check` | Focused tests | Watchdog | VisionX | Storage | Restart | Reorg | Snapshot | State root | Full release | CI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 — Characterization | Required | Required | No | No | No | No | No | No | No | Required | Required |
| 2 — Typed parser seam | Required | Required | No | No | No | No | No | No | No | Required | Required |
| 3 — Runtime threads | Required | Required | Required | Required | No | No | No | No | No | Required | Required |
| 4 — Scalar syntax | Required | Required | Required | Required | No | No | No | No | No | Required | Required |
| 5 — Data directory | Required | Required | No | No | Required | Required | Required | Required | Required | Required | Required |
| 6 — P2P identity/seeds | Required | Required | Required | Required | No | No | No | No | No | Required | Required |
| 7 — Mining | Required | Required | Required | Required | No | No | No | No | Required | Required | Required |
| 8 — Alpha API flag | Required | Required | Required | Required | No | No | No | No | No | Required | Required |
| 9A — Remove file claim | Required | Required | No | No | No | No | No | No | No | Required | Required |
| 9B — Implement file | Required | Required | Required | Required | Required | Required | Conditional | Required | Required | Required | Required |
| 10 — Documentation | No | Link/path checks | No | No | No | No | No | No | No | No | Conditional |

The matrix is subordinate to [TESTING_POLICY.md](TESTING_POLICY.md). If a
future diff crosses another boundary, use the higher-risk gate.

## Rollback Strategy

Configuration Hardening should not require database-format rollback. Each
behavioral commit must be revertible independently.

Operator rollback consists of:

- restoring valid environment values;
- removing a newly adopted configuration file when Path B is selected and the
  prior executable is restored;
- reverting the isolated behavior commit;
- restarting against the same data directory without schema conversion.

If any phase creates or migrates persisted data, stop: it has crossed the
authorized Configuration Hardening boundary and requires OD-08.

## Review Order

Review should proceed in the same order as the commits. Do not batch approval
for unresolved later phases.

The lowest-risk implementation work is Commit 1, configuration
characterization. The lowest-risk behavior change is Commit 3, strict runtime
thread validation, after Commit 2 establishes the parser/error seam.

Configuration Hardening is complete only when:

- invalid input fails before side effects where applicable;
- missing input retains documented defaults;
- valid existing configurations retain their intended behavior;
- every accepted source and precedence is documented;
- unused or unrealized settings have an explicit disposition;
- all required validation passes on the exact candidate;
- operator migration and rollback guidance is published.

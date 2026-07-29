# Testing Policy

## Purpose

Vision-Core testing exists to establish deterministic protocol behavior, state integrity, interoperability, and release fitness. Passing tests are evidence only when the command, revision, environment, and result are recorded.

The current verified baseline is maintained in [CURRENT_STATUS.md](CURRENT_STATUS.md). The consensus risk model is defined in [CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md).

## Test Principles

1. Tests must be deterministic.
2. Consensus behavior must be covered by exact vectors and state-transition assertions.
3. A focused test proves the local behavior; it does not replace the broader suite.
4. An ignored test is visible debt, not a passing test.
5. A test that depends on timing, peer arrival order, global mutable state, or prior test execution must be corrected.
6. Release claims must report exact counts and ignored tests.
7. Validation must run from the revision intended for release, with locked dependencies.

## Test Layers

### Unit tests

Unit tests cover local invariants such as encoding, arithmetic, validation rules, and state transitions. Consensus-sensitive helpers require boundary and failure cases, not only happy paths.

### Module tests

Module suites cover interactions within a subsystem, including P2P synchronization, chain state, storage, mining, and VisionX. Run the relevant module suite for every change to that subsystem.

### Integration and release tests

The full release suite exercises the compiled release configuration and guards against cross-module regressions. It is required before a release candidate is approved.

### VisionX validation

VisionX validation is a distinct gate because proof-of-work compatibility and deterministic cache behavior are consensus-critical. Run its focused suite after any change that can affect hashing, encoding, caching, target arithmetic, concurrency, or dependency behavior.

### Persistence and restart tests

Changes to state structures, storage, snapshots, initialization, or recovery require tests that:

- create and persist representative state;
- restart from persisted state;
- reconstruct the same chain and committed state;
- reject corrupt or incompatible data safely;
- preserve metadata and cumulative work.

### Reorganization tests

Chain-state and fork-choice changes require tests for:

- competing branches;
- cumulative-work selection;
- disconnect and reconnect behavior;
- UTXO restoration;
- deep reorganization where supported;
- deterministic resulting tip and state root.

### Snapshot and state-root tests

Changes that can affect state commitment require deterministic state-root checks before and after snapshot round trips. Import must validate the snapshot rather than trusting serialized metadata.

### Network tests

P2P changes require focused tests for handshake compatibility, message bounds, synchronization progress, invalid-peer handling, watchdog recovery, and deterministic peer selection in the test environment.

## Validation Matrix

| Change class | Minimum validation |
| --- | --- |
| Documentation only | Link/path checks, `git diff --check`, docs-only scope audit |
| Formatting only | `cargo fmt --all -- --check`, diff review proving no logical changes, `cargo check --locked`, full release suite |
| Test-infrastructure-only | Formatting check, `cargo check --locked`, focused test, directly related module tests, full release suite when changing shared harness behavior |
| Warning or dead-code cleanup | Formatting check, `cargo check --locked`, focused tests, affected module tests, warning-count comparison, full release suite |
| Local non-consensus Rust | Formatting check, `cargo check --locked`, focused tests, affected module tests, full release suite |
| Configuration | Formatting check, `cargo check --locked`, valid and invalid startup cases, configuration-focused tests, focused watchdog, VisionX suite, full release suite |
| P2P or synchronization | Local baseline plus handshake/synchronization tests, focused watchdog, relevant multi-node tests, VisionX suite, full release suite |
| VisionX or proof of work | Exact historical vectors, mining/verification tests, full release suite, VisionX suite, compatibility review |
| Chain state or persistence | Storage tests, restart, reorganization, snapshot/state-root validation, full release suite |
| Consensus-sensitive | All applicable gates plus exact vectors, explicit consensus review, compatibility evidence, and owner authorization |
| Protocol compatibility | Encoding and handshake vectors, P2P/watchdog/multi-node tests, full release suite, compatibility review |
| Release candidate | Locked release suite, focused watchdog, VisionX, applicable state/persistence suites, repository audit, clean-clone audit, CI |

The matrix is a floor. Reviewers may require more evidence based on the affected call graph and state model.

## Standard Command Discipline

Use the repository’s pinned toolchain and locked dependency graph. Prefer:

```powershell
cargo check --all-targets --locked
cargo test --release --locked -- --test-threads=1
```

Use `--offline` only when the required dependency cache is known to be complete. Use a dedicated `CARGO_TARGET_DIR` when isolating concurrent or historical validation runs. Do not interpret a failure caused by an unavailable dependency as a code failure; report the environmental limitation separately.

Focused filters must be precise enough to show the intended test ran. Record the number of matched tests.

## Validation Workflow

### 1. Classify the change

Identify every affected boundary: documentation, tests, application behavior,
configuration, API, networking, persistence, state, proof of work, consensus,
and protocol compatibility. Use the highest-risk applicable row in the matrix.

### 2. Establish the starting state

Record:

```powershell
git status --short --branch
git rev-parse HEAD
rustc --version
cargo --version
```

Do not attribute pre-existing worktree changes or warnings to the current task.

### 3. Run the narrowest useful test first

The focused regression should prove the intended behavior and fail for the
original defect. Confirm that the filter discovers the expected test count.

### 4. Expand by subsystem

Run affected module and integration tests. Persistence work expands to restart,
reorganization, snapshot, and state-root tests. Networking work expands to
synchronization, watchdog, and applicable multi-node tests.

### 5. Run the release gate

Run the locked release suite after focused evidence is green. Release-mode
success matters because optimization, timing, and conditional compilation can
differ from debug behavior.

### 6. Verify repository hygiene

```powershell
cargo fmt --all -- --check
git diff --check
git status --short --branch
```

Review the complete diff and confirm that only authorized files and concerns
changed.

### 7. Record evidence

Record exact commands, counts, ignored tests, warning transitions,
environmental deviations, and the tested commit or uncommitted tree.

## Required Evidence Record

```text
Candidate:
Branch:
Worktree state:
Toolchain:

Change classification:
Consensus impact:
Protocol impact:
Persistence impact:
Runtime/API impact:

Command:
Result:
Passed:
Failed:
Ignored:
Warnings:

Limitations or deviations:
```

Never report “tests pass” without identifying which tests ran.

## Determinism Requirements

Tests must not require:

- a particular global test order;
- residue from a prior run;
- an uncontrolled network service;
- arbitrary sleeps as the primary synchronization mechanism;
- nondeterministic peer ordering;
- filesystem paths outside an isolated temporary workspace;
- shared mutable caches without synchronization and cleanup.

Where time participates in the behavior, use controlled deadlines, observable state transitions, or injected clocks where the architecture permits. A regression test must fail for the original defect for the reason the test name claims.

## Ignored Tests

Each ignored test must have:

- a documented reason;
- a named owner or subsystem;
- a condition for restoration or retirement;
- visibility in release evidence.

Do not increase the ignored-test count during cleanup without explicit approval. The current ignored test is identified in [CURRENT_STATUS.md](CURRENT_STATUS.md).

## Failure Handling

When a test fails:

1. Preserve the exact command and output.
2. Re-run only as needed to determine determinism.
3. Distinguish product failures from environment, resource, and dependency failures.
4. Do not weaken or delete the assertion merely to restore green status.
5. Add the smallest regression test that captures the defect.
6. Run the focused test, affected module suite, and required broader gates.

Flaky behavior is a defect. Repeated passing runs can support a determinism claim but do not excuse an unexplained failure.

## Release Evidence

A release validation record must include:

- exact commit;
- dirty or clean worktree status;
- Rust toolchain;
- commands executed;
- pass, fail, and ignored counts;
- focused watchdog result;
- VisionX result;
- persistence, restart, reorganization, and state-root results when applicable;
- `git diff --check`;
- any limitations or deviations.

Counts in prose are historical evidence and must not be copied forward without rerunning the suite.

## CI Policy

Local validation is required before relying on CI. CI is an independent
environment and promotion gate, not a substitute for understanding a local
failure.

When CI differs from local results:

1. compare the exact commit and dependency lockfile;
2. compare toolchain, operating system, features, and environment;
3. preserve the CI logs;
4. reproduce the narrowest failing command locally where possible;
5. classify infrastructure failures separately from product failures;
6. do not rerun repeatedly to conceal nondeterminism.

Changing CI to make a product failure disappear is a separate engineering
concern and requires its own review.

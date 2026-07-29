# Vision-Core Phase II Entry Audit

## Executive Summary

The Vision Engineering Foundation is technically coherent and its branch
history is linearly promotable. The repository is not yet ready to begin
Configuration Hardening because the final documentation state is uncommitted,
the review branch has not been pushed, remote CI has not validated that state,
and `main` has not been promoted.

No technical defect was found that must be repaired before review. The
remaining gates are administrative and governance gates already required by
the repository operating system.

Final classification:

**B. Repository ready after review-branch push and CI**
## Audit Authority and Scope

The audit began with repository `AGENTS.md` and followed its mandatory reading
order. Configuration, release, dead-code, decision-record, and documentation
audit materials were then read as task-specific authorities.

The audit was read-only except for the three requested Markdown deliverables:

- `CONFIGURATION_HARDENING_INVENTORY.md`;
- `CONFIGURATION_HARDENING_IMPLEMENTATION_PLAN.md`;
- `PHASE_II_ENTRY_AUDIT.md`.

No Rust, Cargo, tests, CI, commits, branches, tags, merges, or remote refs were
modified.

## Repository Verification

### Current state

| Check | Result |
| --- | --- |
| Current branch | `dev/dead-code-cleanup-v104` |
| Current committed HEAD | `b83240dec488726c896353f34b25f8f3600b6859` |
| Current committed tree | `974f928832630a53830583a5847643e62ba49885` |
| Worktree | Dirty; documentation-only tracked and untracked changes |
| Non-documentation worktree changes | 0 |
| Current release tag | `vision-core-consensus-v1.0.4` |
| Annotated tag object | `c1a0cf7e71414703e24365c25f6635cd9acba594` |
| Peeled release commit | `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Release tree | `d650bb3419db56cce9e1d789611763e5cb4cbc26` |
| Live remote `main` | `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Development branch versus remote `main` | 15 ahead, 0 behind |
| Merge base | `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Remote `main` ancestor of HEAD | Yes |
| Merge commits after remote `main` | 0 |
| History shape | Linear fast-forward candidate |
| Remote review branch | Not present at audit time |

The worktree has no single committed tree representing the final foundation
documentation. The committed tree above identifies `HEAD`; the documentation
changes remain outside that tree.

### Development commit sequence

The 15 commits after v1.0.4 are linear:

1. `f4e626a` — Add Vision-Core developer documentation
2. `40f24d9` — Document currently validated Rust toolchain
3. `98908e7` — Align node release identity with v1.0.4
4. `4d92abf` — Repair corrupted source documentation encoding
5. `2d06a17` — Add baseline build and release-test CI
6. `2e634f5` — Apply repository-wide Rust formatting
7. `f9e50d5` — Remove unambiguous unused imports and test mutability
8. `c8adc05` — Modernize multi-node temporary directory handling
9. `f31bd6b` — Resolve trivial test-only Clippy findings
10. `1c2dfa7` — Establish formatting and lint quality baseline
11. `89a5616` — Classify unused code and deletion prerequisites
12. `8dcab2b` — Remove unused bootstrap test helper
13. `56e923d` — Remove unused multi-node API address field
14. `d63c7d0` — Restore single-block cumulative-work test
15. `b83240d` — Remove obsolete ChainState mempool fields

The branch stack is broader than documentation: earlier approved commits
include toolchain, package identity, CI, formatting, warning cleanup,
test-infrastructure cleanup, and ChainState cleanup. The Engineering Foundation
documentation currently present in the worktree is documentation-only.

## Documentation Verification

### Reading order and existence

Every document in the `AGENTS.md` mandatory reading order exists:

- Project Charter;
- Project Vision;
- Current Status;
- Architecture Overview;
- Engineering Manifest;
- Engineering Principles;
- Consensus Boundaries;
- Testing Policy;
- Coding Standards;
- Engineering Playbook;
- accepted engineering decision records.

The task-specific Release Process, Configuration, Dead-Code Ledger,
Documentation Consistency Audit, Founder Vision, and documentation index also
exist.

### Inventory

Before the three Phase II deliverables, the repository contained 37 Markdown
documents. After this audit it contains 40.

The inventory covers:

- repository entry, contribution, and security guidance;
- project purpose, founder intent, vision, charter, and engineering values;
- current status, architecture, API, configuration, history, and roadmap;
- consensus, testing, release, and coding policies;
- playbooks, glossary, quality baseline, and dead-code ledger;
- eight accepted engineering decision records;
- documentation consistency audit and navigation index;
- the Phase II inventory, plan, and entry audit.

All local Markdown file links and heading anchors resolved before the Phase II
deliverables were written. Final link validation includes the three new files.

### Documentation audit status

`DOCUMENTATION_CONSISTENCY_AUDIT.md` classifies the audited foundation as:

**A — Production-quality engineering documentation**

That audit covered 34 documents. Founder Vision and `INDEX.md` were added
afterward. This Phase II audit reviewed those additions and found them
consistent with the Charter, Manifest, and policy authority model.

### Policy authority

Each engineering policy has one authoritative source:

| Policy | Authority |
| --- | --- |
| Current repository facts | `CURRENT_STATUS.md` |
| Consensus/protocol classification and authorization | `CONSENSUS_BOUNDARIES.md` |
| Validation requirements and evidence | `TESTING_POLICY.md` |
| Release Promotion, tagging, publication, and failed candidates | `RELEASE_PROCESS.md` |
| Rust, module, error, documentation, and commit conventions | `CODING_STANDARDS.md` |
| Recurring workflows | `ENGINEERING_PLAYBOOK.md` |
| Unused-code disposition | `DEAD_CODE_LEDGER.md` |
| Accepted rationale | `DECISIONS/` |

`AGENTS.md` routes to these authorities and does not redefine their detailed
rules. `INDEX.md` explicitly states that it is navigation rather than policy.

### State consistency

Documentation agrees on:

- v1.0.4 as the authoritative release;
- `b874d73cbdf60657334b62c867ed7f18b80a186b` as release and remote-main commit;
- `dev/dead-code-cleanup-v104` as the development branch;
- `b83240dec488726c896353f34b25f8f3600b6859` as committed development HEAD;
- 506 tests discovered, 505 passed, 0 failed, and 1 ignored in the recorded
  post-Tranche 3 validation;
- 58 normal-target and 31 test-target warnings currently recorded;
- 58 normal-target and 34 test-target warnings as the historical Tranche 2
  baseline;
- Developer Readiness promotion as the current stage;
- Configuration Hardening as the gated next engineering task;
- all unresolved dead-code ledger entries remaining frozen.

No contradiction with current Git state was found.

## Repository Readiness by Operation

| Operation | Ready now? | Required before operation |
| --- | --- | --- |
| Review-branch push | No | Review and commit the documentation-only work under explicit authorization; verify final commit and clean worktree; authorize push. |
| Remote CI | No | Push the exact review commit. GitHub Actions cannot validate uncommitted local files. |
| Pull request | No | Push the branch, confirm remote branch identity, open the PR under authorization, and allow checks to run. |
| Promotion to `main` | No | Successful CI, complete diff and evidence review, explicit owner authorization, and a final remote-main ancestry check. |
| Begin Configuration Hardening | No | Promote Developer Readiness, verify updated `main`, and create a fresh branch from it. |

The branch ancestry itself is ready for a normal fast-forward. Readiness is
blocked by the uncommitted documentation state and required review workflow,
not by divergent history.

## Blockers

### Technical

No technical blocker was identified for review-branch preparation.

The recorded Rust validation predates the final documentation additions, but
the additions are documentation-only and the Testing Policy requires
documentation link/path checks, `git diff --check`, and a documentation-only
scope audit rather than a Rust suite.

### Governance

- Creating the documentation commit requires authorization not granted by this
  audit request.
- Pushing the review branch requires explicit authorization.
- Opening a pull request and promoting `main` are external/repository actions
  that require authorization.
- The proposed engineering-foundation tag may be considered only after
  promotion and requires separate exact tag/commit authorization.

### Administrative

- The worktree is dirty with the completed documentation foundation.
- The final documentation state has no commit or tree identity.
- `dev/dead-code-cleanup-v104` is not present on the remote.
- GitHub Actions has not evaluated the final foundation state.
- No pull request exists for the final state within the evidence available to
  this audit.

### Owner Decision Required

The twelve central owner decisions in `CURRENT_STATUS.md` remain open. They do
not block review or promotion of the Developer Readiness stack.

The following decisions specifically gate later Configuration Hardening
phases:

- OD-09: configuration-file precedence and operator migration;
- whether the permissive private-peer default is retained;
- whether mining requires an explicit nonzero reward address;
- whether `VISION_MINING_THREADS` is implemented, renamed, or removed;
- whether `VISION_CONFIG` is implemented or its unrealized claim is removed;
- empty data-directory and explicitly empty seed-list policy;
- whether bind hosts become configurable;
- whether `RUST_LOG` and `TOKIO_WORKER_THREADS` join the strict configuration
  contract.

These do not block the first characterization commit. They must be resolved
before their dependent behavior commits.

## Configuration Hardening Readiness

The source audit found:

- eleven operator `VISION_*` settings in `Settings`;
- no command-line configuration;
- no configuration-file loader;
- five production environment variables outside `Settings`, counting
  `VISION_GIT_COMMIT` and `GIT_COMMIT` separately;
- seven test-only environment controls;
- compile-time consensus and policy constants;
- active hard-coded bind hosts, timers, message bounds, and cache capacity;
- four repeated direct `Settings` constructors in tests;
- only seed-peer parsing covered by direct settings unit tests.

The detailed findings and validation consequences are in
`CONFIGURATION_HARDENING_INVENTORY.md`.

The implementation plan starts with characterization, then a source-neutral
typed parser seam, followed by isolated behavior changes. It keeps
configuration files, persistence selection, P2P identity, mining, and API
enablement in separate commits.

No configuration implementation was started.

## Promotion Audit Requirements

The next authorized promotion audit should:

1. review all documentation diffs, including the three Phase II deliverables;
2. verify all links and anchors;
3. run `git diff --check`;
4. confirm zero non-documentation worktree changes;
5. create an isolated documentation commit under authorization;
6. record its commit and tree;
7. confirm the branch remains 0 behind remote `main`;
8. push only the review branch;
9. inspect GitHub Actions results for the exact pushed commit;
10. review the full 16-commit candidate diff from `main`;
11. request explicit fast-forward authorization;
12. verify remote `main` after promotion.

The original 15-commit branch is linear. The future documentation commit would
make the candidate 16 commits ahead if no other remote change occurs.

## Final Assessment

Vision-Core has completed the Engineering Foundation at the working-tree level.
Its operating model, policies, history, purpose, and Phase II configuration
plan are sufficiently mature.

The repository must now convert the local documentation state into an exact
review commit, validate it remotely, and promote it through the documented
workflow. Configuration Hardening begins only afterward on a fresh branch.

## Final Classification

**B. Repository ready after review-branch push and CI**

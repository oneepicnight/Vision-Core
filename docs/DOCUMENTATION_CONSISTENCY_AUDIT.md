# Vision-Core Documentation Consistency Audit

## Executive Summary

The Vision-Core engineering documentation was audited against the current
worktree, local Git objects, fetched remote-tracking refs, source-backed
repository facts, historical tranche evidence, and the documentation authority
model.

The audit covered all 34 Markdown documents that existed before this report was
created. It found no broken local Markdown links, invalid documented commit
hashes, incorrect release identities, incorrect branch names, or contradictions
between the Project Charter and Engineering Manifest.

Six conflicting or stale factual statements were found:

- one command-level inconsistency in the generic testing examples;
- four resolved dead-code ledger entries still described as pending;
- one README reference that presented a historical warning baseline as the
  place to find current counts.

All six were corrected. Additional editorial consolidation removed duplicate
policy ownership, completed the mandatory reading order, centralized unresolved
owner decisions, and standardized named initiative terminology.

The resulting documentation has a clear authority model:

- `CURRENT_STATUS.md` owns current repository facts;
- `CONSENSUS_BOUNDARIES.md` owns protected-boundary classification and
  authorization;
- `TESTING_POLICY.md` owns minimum validation and evidence;
- `RELEASE_PROCESS.md` owns release governance;
- `CODING_STANDARDS.md` owns implementation and commit conventions;
- `ENGINEERING_PLAYBOOK.md` owns recurring workflows;
- `AGENTS.md` routes contributors to those authorities.

Final classification: **A — Production-quality engineering documentation**.

## Audit Scope and Method

The audit used:

- `rg --files -g '*.md'` for the complete documentation inventory;
- local Markdown-link resolution for every repository Markdown file;
- UTF-8, tab, and trailing-whitespace scans;
- Git object verification for documented commits and trees;
- local annotated-tag and peeled-tag verification;
- fetched `origin/main`, `origin/HEAD`, and archival-ref verification;
- source inspection for the four completed Tranche 3 dispositions;
- cross-document searches for versions, branches, commits, test counts,
  warning counts, named initiatives, protected subsystems, validation rules,
  release operations, and owner decisions;
- manual reconciliation of the policy, architecture, roadmap, history,
  charter, manifest, playbook, decision records, and `AGENTS.md`.

Live network access to GitHub was unavailable during the audit. Remote-state
claims were therefore checked against the repository’s fetched remote-tracking
refs. This limitation does not affect local tag objects, commit ancestry,
document consistency, or the documentation-only readiness classification. A
live remote audit remains part of Release Promotion under
`RELEASE_PROCESS.md`.

## Documents Audited

### Repository entry and governance

1. `AGENTS.md`
2. `README.md`
3. `CONTRIBUTING.md`
4. `SECURITY.md`

### Project intelligence and operations

5. `docs/API.md`
6. `docs/ARCHITECTURE.md`
7. `docs/ARCHITECTURE_OVERVIEW.md`
8. `docs/CODING_STANDARDS.md`
9. `docs/CONFIGURATION.md`
10. `docs/CONSENSUS_BOUNDARIES.md`
11. `docs/CONSENSUS_CHANGE_POLICY.md`
12. `docs/CURRENT_STATUS.md`
13. `docs/DEAD_CODE_LEDGER.md`
14. `docs/DEVELOPMENT_HISTORY.md`
15. `docs/ENGINEERING_PLAYBOOK.md`
16. `docs/ENGINEERING_PRINCIPLES.md`
17. `docs/GLOSSARY.md`
18. `docs/LICENSE_DECISION_REQUIRED.md`
19. `docs/PROJECT_CHARTER.md`
20. `docs/PROJECT_VISION.md`
21. `docs/QUALITY_BASELINE.md`
22. `docs/RELEASE_PROCESS.md`
23. `docs/ROADMAP.md`
24. `docs/TESTING_POLICY.md`
25. `docs/VISION_ENGINEERING_MANIFEST.md`

### Engineering decision records

26. `docs/DECISIONS/README.md`
27. `docs/DECISIONS/0001_consensus_preservation.md`
28. `docs/DECISIONS/0002_vpow_encoding.md`
29. `docs/DECISIONS/0003_release_identity.md`
30. `docs/DECISIONS/0004_deterministic_visionx_cache.md`
31. `docs/DECISIONS/0005_cumulative_work_fork_choice.md`
32. `docs/DECISIONS/0006_state_root_and_persistence_integrity.md`
33. `docs/DECISIONS/0007_deterministic_watchdog_testing.md`
34. `docs/DECISIONS/0008_developer_quality_baseline.md`

This report is the thirty-fifth Markdown document after creation and is not
counted as an input to its own audit.

## Verified Repository State

| Fact | Verified value |
| --- | --- |
| Authoritative release | Vision-Core Consensus v1.0.4 |
| Release tag | `vision-core-consensus-v1.0.4` |
| Release commit | `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Release tree | `d650bb3419db56cce9e1d789611763e5cb4cbc26` |
| Fetched authoritative branch | `origin/main` at `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Preserved archival ref | `origin/archive/main-pre-v103-032a0f2` at `032a0f2da4712b24226d561b6b9f6463c2935ce3` |
| Development branch | `dev/dead-code-cleanup-v104` |
| Current committed HEAD | `b83240dec488726c896353f34b25f8f3600b6859` |
| Current committed tree | `974f928832630a53830583a5847643e62ba49885` |
| Package version | `1.0.4` |
| Pinned Rust toolchain | `1.97.1` |
| Current test baseline | 506 discovered; 505 passed; 0 failed; 1 ignored |
| Current warning baseline | 58 normal-target; 31 test-target |
| Historical Tranche 2 baseline | 58 normal-target; 34 test-target |
| Current roadmap stage | Developer Readiness promotion, then Configuration Hardening |
| Planned milestone | Vision-Core v1.1.0; exact scope Owner Decision Required |

The current test and warning totals are validation evidence from the completed
Tranche 3 work. This documentation audit did not rerun Rust validation because
no Rust, Cargo, CI, or test files were changed.

## Issues Found

### Factual conflicts

1. `TESTING_POLICY.md` showed generic commands that omitted `--all-targets` for
   `cargo check` and omitted the established single-threaded argument for the
   release suite.
2. `DEAD_CODE_LEDGER.md` still described `NODE-02` as pending after removal.
3. `DEAD_CODE_LEDGER.md` still described `TEST-01` as pending after removal.
4. `DEAD_CODE_LEDGER.md` still described `CHAIN-04` as awaiting disposition
   after its test annotation was restored.
5. `DEAD_CODE_LEDGER.md` still described `CHAIN-05` as awaiting audit after its
   approved fields were removed.
6. `README.md` directed readers to the historical Tranche 2 warning document
   without identifying `CURRENT_STATUS.md` as the current-count authority.

### Policy duplication and authority ambiguity

- `AGENTS.md` repeated protected-subsystem and validation details instead of
  routing to their controlling policies.
- `CONSENSUS_BOUNDARIES.md`, `TESTING_POLICY.md`, and
  `RELEASE_PROCESS.md` each carried a slightly different validation list.
- The older `CONSENSUS_CHANGE_POLICY.md` duplicated rules superseded by
  `CONSENSUS_BOUNDARIES.md`.
- `CONTRIBUTING.md` repeated validation and consensus rules instead of linking
  to the policy authorities.
- `CODING_STANDARDS.md` repeated validation commands owned by
  `TESTING_POLICY.md`.
- `QUALITY_BASELINE.md` mixed historical evidence with current policy language.

### Navigation and terminology

- `AGENTS.md` did not include the Project Charter, Engineering Manifest, or
  Engineering Playbook in its mandatory reading order.
- Named initiative capitalization varied for Developer Readiness and Release
  Promotion.
- The planned v1.1.0 milestone appeared in the Engineering Manifest but not the
  Roadmap.
- Owner Decision Required items were distributed across several documents
  without a central numbered register.

### Issues not found

- No broken local Markdown links.
- No missing referenced documents.
- No invalid documented commit hashes.
- No incorrect tree hashes.
- No incorrect release tag targets.
- No incorrect current branch or HEAD.
- No incorrect current test or warning totals after distinguishing historical
  and current baselines.
- No conflicting consensus boundaries.
- No conflict between the Project Charter and Engineering Manifest.
- No conflict between the Roadmap and Development History after the v1.1.0
  milestone was classified as planned.
- No workflow in the Engineering Playbook points to an obsolete authority.

## Corrections Made

Fifteen correction groups were applied across seventeen existing documentation
files:

1. Corrected the authoritative generic Cargo check and release-suite examples.
2. Marked all four completed dead-code dispositions as resolved with commits
   and evidence.
3. Clarified that ledger classification totals are historical and that
   remaining entries are frozen.
4. Distinguished the historical 58/34 Tranche 2 warning baseline from the
   current 58/31 post-Tranche 3 baseline.
5. Corrected README navigation to current and historical warning authorities.
6. Reworked `AGENTS.md` into a mandatory router with a single policy-ownership
   table.
7. Added the Charter, Manifest, and Playbook to the mandatory reading order.
8. Made `TESTING_POLICY.md` the single validation-matrix authority.
9. Removed duplicate validation gates from `CONSENSUS_BOUNDARIES.md` and
   `RELEASE_PROCESS.md`.
10. Converted `CONSENSUS_CHANGE_POLICY.md` into a stable compatibility pointer
    to the current authorities.
11. Converted `CONTRIBUTING.md` into a contributor workflow that references
    controlling policy rather than restating it.
12. Removed duplicate validation commands from `CODING_STANDARDS.md` and
    current-policy claims from the historical quality baseline.
13. Centralized twelve unresolved owner decisions in `CURRENT_STATUS.md`.
14. Added terminology conventions and definitions for the named engineering
    initiatives.
15. Added planned v1.1.0 to the Roadmap and clarified that the Charter,
    Manifest, and Principles are purpose and rationale documents rather than
    competing operational policies.

No engineering philosophy was rewritten. The corrections establish ownership,
history, and navigation while preserving author intent.

## Remaining Owner Decisions

Twelve unresolved decision groups remain, centrally registered as OD-01 through
OD-12 in `CURRENT_STATUS.md`:

1. repository license;
2. private security-reporting channel;
3. supported Rust library API policy;
4. GitHub Action pinning and dependency-update policy;
5. future dead-code authorization and frozen-ledger ownership;
6. consensus and protocol activation governance;
7. genesis and new-network migration policy;
8. database migration, downgrade, and rollback policy;
9. Configuration Hardening precedence and operator migration details;
10. recovery after a failed candidate has already been promoted;
11. emergency chain and database incident recovery;
12. exact Vision-Core v1.1.0 scope and compatibility classification.

These decisions are explicitly unresolved. None blocks documentation readiness.
They block only the work governed by the relevant decision.

## Documentation Quality Assessment

### Consistency

Current state is centralized and internally consistent. Historical baselines
are labeled as historical. Completed ledger dispositions match source and Git
history.

### Authority and duplication

Operational ownership is explicit. Supporting documents may explain reasons or
show historical evidence, but they reference rather than redefine controlling
rules.

### Traceability

Release identity, development HEAD, tree hashes, tranche commits, decision
records, test counts, warning counts, and owner decisions are traceable.

### Contributor usability

A new contributor has:

- a mandatory entry point;
- project purpose and engineering values;
- current status;
- architecture and subsystem boundaries;
- consensus classification;
- validation policy;
- release lifecycle;
- coding and commit standards;
- recurring playbooks;
- decision history;
- a roadmap that distinguishes completed, approved, planned, and visionary
  work.

### Open-source infrastructure maturity

The repository now documents the operational concerns expected of mature
infrastructure: compatibility, deterministic testing, persistence, recovery,
release identity, failed candidates, security reporting limitations, licensing
status, and owner authorization.

## Documentation Completeness

The engineering documentation phase is complete for the current implemented
system and approved next task.

Known incompleteness is deliberate and visible:

- no license has been selected;
- no authoritative security inbox exists;
- the HTTP API lacks a stability and OpenAPI policy;
- database migration policy remains undecided;
- operator deployment and incident runbooks remain future engineering work;
- Configuration Hardening has not yet produced its implementation-specific
  migration notes;
- v1.1.0 scope is not yet approved.

Those are product, governance, or future-feature decisions, not missing
explanations of current engineering truth.

## Recommended Improvements

Do not add more general project-description documents.

Maintain the existing set as living documentation:

1. update `CURRENT_STATUS.md` whenever a promoted commit changes repository
   truth;
2. update the Roadmap only when owner intent or implementation status changes;
3. add an ADR for material architecture, compatibility, or governance
   decisions;
4. update API, configuration, persistence, and operator documentation with the
   features that change them;
5. rerun the documentation consistency checks before each release;
6. resolve OD-01 through OD-12 only through explicit owner decisions;
7. keep the audit report as a historical point-in-time assessment rather than
   silently rewriting its findings.

The next work should be the documented promotion audit, CI validation, Release
Promotion, and then Configuration Hardening—not another broad documentation
expansion.

## Final Readiness Classification

**A — Production-quality engineering documentation**

The documentation is coherent, navigable, evidence-backed, operationally
useful, and sufficiently complete for the current repository state. Remaining
decisions are explicit and correctly scoped.

## Final Counts

| Measure | Count or result |
| --- | --- |
| Total documents audited | 34 |
| Correction groups made | 15 |
| Existing documents corrected | 17 |
| Broken local Markdown links | 0 |
| Conflicting or stale factual statements found | 6 |
| Conflicting statements remaining | 0 |
| Owner decision groups remaining | 12 |
| New files created by this audit | 1 |
| Non-documentation files modified | 0 |
| Repository status | Dirty; documentation-only tracked and untracked changes |
| Current branch | `dev/dead-code-cleanup-v104` |
| Current HEAD | `b83240dec488726c896353f34b25f8f3600b6859` |
| Commits created | 0 |
| Branches, tags, or remote refs changed | 0 |

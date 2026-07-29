# Vision-Core Engineering Documentation Index

This page is the navigation guide for the Vision-Core engineering knowledge
base. It does not define policy. Follow the linked controlling documents.

## Start Here

| Situation | Read first |
| --- | --- |
| You are uncertain what applies | [AGENTS.md](../AGENTS.md) |
| You are beginning any engineering task | [AGENTS.md](../AGENTS.md), then [Current Status](CURRENT_STATUS.md) |
| You are a new contributor | [New Contributor Path](#new-contributor-path) |
| You need the current release, branch, commit, tests, or warnings | [Current Status](CURRENT_STATUS.md) |
| You need to classify a proposed change | [Consensus Boundaries](CONSENSUS_BOUNDARIES.md), then [Testing Policy](TESTING_POLICY.md) |
| You need a recurring workflow | [Engineering Playbook](ENGINEERING_PLAYBOOK.md) |
| You need the reason behind an accepted design | [Engineering Decision Records](DECISIONS/README.md) |

## New Contributor Path

Read in this order:

1. [Repository Operating Contract](../AGENTS.md)
2. [Project Charter](PROJECT_CHARTER.md)
3. [Founder Vision](FOUNDER_VISION.md)
4. [Project Vision](PROJECT_VISION.md)
5. [Current Status](CURRENT_STATUS.md)
6. [Architecture Overview](ARCHITECTURE_OVERVIEW.md)
7. [Engineering Manifest](VISION_ENGINEERING_MANIFEST.md)
8. [Engineering Principles](ENGINEERING_PRINCIPLES.md)
9. [Consensus Boundaries](CONSENSUS_BOUNDARIES.md)
10. [Testing Policy](TESTING_POLICY.md)
11. [Coding Standards](CODING_STANDARDS.md)
12. [Engineering Playbook](ENGINEERING_PLAYBOOK.md)
13. the relevant [Engineering Decision Records](DECISIONS/README.md)

Then read the subsystem-specific documents for the task.

## By Engineering Task

### Changing consensus, proof of work, or VisionX

Read:

1. [Consensus Boundaries](CONSENSUS_BOUNDARIES.md)
2. [Testing Policy](TESTING_POLICY.md)
3. [Architecture Overview](ARCHITECTURE_OVERVIEW.md)
4. [Implemented Architecture](ARCHITECTURE.md)
5. [Consensus Preservation Decision](DECISIONS/0001_consensus_preservation.md)
6. [Historical VPoW Encoding Decision](DECISIONS/0002_vpow_encoding.md)
7. [Deterministic VisionX Cache Decision](DECISIONS/0004_deterministic_visionx_cache.md)
8. [Cumulative-Work Fork-Choice Decision](DECISIONS/0005_cumulative_work_fork_choice.md)
9. [Protocol-Change Safety Workflow](ENGINEERING_PLAYBOOK.md#evaluating-protocol-change-safety)

Explicit owner authorization is required before implementing protected behavior
changes.

### Preparing or promoting a release

Read:

1. [Current Status](CURRENT_STATUS.md)
2. [Release Process](RELEASE_PROCESS.md)
3. [Testing Policy](TESTING_POLICY.md)
4. [Release Identity Decision](DECISIONS/0003_release_identity.md)
5. [Preparing a Release Workflow](ENGINEERING_PLAYBOOK.md#preparing-a-release)
6. [Post-Release Validation Workflow](ENGINEERING_PLAYBOOK.md#conducting-post-release-validation)

Release Promotion, annotated tag creation, publication, force pushes, history
rewrites, and branch retirement require their own explicit authorization.

### Working on networking or synchronization

Read:

1. [Architecture Overview: Networking](ARCHITECTURE_OVERVIEW.md#networking-architecture)
2. [Implemented Architecture](ARCHITECTURE.md)
3. [Consensus Boundaries: Networking](CONSENSUS_BOUNDARIES.md#networking)
4. [Testing Policy](TESTING_POLICY.md)
5. [Deterministic Watchdog Testing Decision](DECISIONS/0007_deterministic_watchdog_testing.md)
6. [Dead-Code Ledger: Node and P2P Findings](DEAD_CODE_LEDGER.md#node-and-p2p-findings)

Networking changes must preserve unified block validation, deterministic
synchronization state transitions, compatibility identity, watchdog recovery,
and the rule that advertised work is not authority.

### Modifying persistence, snapshots, or state roots

Read:

1. [Architecture Overview: Storage](ARCHITECTURE_OVERVIEW.md#storage-persistence-and-recovery)
2. [Consensus Boundaries: State Root](CONSENSUS_BOUNDARIES.md#state-root)
3. [Consensus Boundaries: Persistence](CONSENSUS_BOUNDARIES.md#persistence)
4. [State-Root and Persistence Integrity Decision](DECISIONS/0006_state_root_and_persistence_integrity.md)
5. [Testing Policy](TESTING_POLICY.md)
6. [Dead-Code Ledger: Chain, State, and Persistence](DEAD_CODE_LEDGER.md#chain-state-and-persistence-findings)

Persistence-format, migration, downgrade, rollback, and state-root algorithm
changes require explicit owner decisions.

### Changing configuration

Follow:

1. [Current Configuration](CONFIGURATION.md)
2. [Current Status: Approved Next Task](CURRENT_STATUS.md#approved-next-task-configuration-hardening)
3. [Consensus Boundaries: Configuration](CONSENSUS_BOUNDARIES.md#configuration)
4. [Configuration Hardening Workflow](ENGINEERING_PLAYBOOK.md#configuration-hardening-workflow)
5. [Testing Policy](TESTING_POLICY.md)
6. [Roadmap: Configuration Hardening](ROADMAP.md#configuration-hardening)
7. [Dead-Code Ledger: Configuration and Policy Constants](DEAD_CODE_LEDGER.md#configuration-and-policy-constants)

Configuration Hardening changes observable startup behavior. It begins only
after the Developer Readiness stack has completed review, CI, and Release
Promotion.

### Changing the HTTP API

Read:

1. [HTTP API](API.md)
2. [Architecture Overview: Application and API Boundary](ARCHITECTURE_OVERVIEW.md#application-and-api-boundary)
3. [Coding Standards: APIs and Configuration](CODING_STANDARDS.md#apis-and-configuration)
4. [Testing Policy](TESTING_POLICY.md)
5. [Roadmap: API Cleanup](ROADMAP.md#api-cleanup)

The API is not currently declared stable. Do not invent a compatibility promise
or bypass Core validation through an application route.

### Changing Rust code or module structure

Read:

1. [Coding Standards](CODING_STANDARDS.md)
2. [Architecture Overview](ARCHITECTURE_OVERVIEW.md)
3. [Implemented Architecture](ARCHITECTURE.md)
4. [Consensus Boundaries](CONSENSUS_BOUNDARIES.md)
5. [Testing Policy](TESTING_POLICY.md)

### Performing warning or dead-code cleanup

Read:

1. [Current Status](CURRENT_STATUS.md)
2. [Historical Quality Baseline](QUALITY_BASELINE.md)
3. [Dead-Code Ledger](DEAD_CODE_LEDGER.md)
4. [Developer Quality Baseline Decision](DECISIONS/0008_developer_quality_baseline.md)
5. [Dead-Code Audit Workflow](ENGINEERING_PLAYBOOK.md#performing-a-dead-code-audit)
6. [Testing Policy](TESTING_POLICY.md)

All unresolved ledger items are frozen. A compiler warning is not deletion
authorization.

### Reviewing a pull request

Use:

1. [Pull Request Review Workflow](ENGINEERING_PLAYBOOK.md#reviewing-a-pull-request)
2. [Consensus Boundaries](CONSENSUS_BOUNDARIES.md)
3. [Testing Policy](TESTING_POLICY.md)
4. [Coding Standards: Review Readiness](CODING_STANDARDS.md#review-readiness)
5. [Contributing Guide](../CONTRIBUTING.md)

Confirm scope, authorization, commit isolation, compatibility, exact candidate
evidence, documentation updates, and unresolved owner decisions.

### Investigating a defect or security issue

Read:

1. [Security Policy](../SECURITY.md)
2. [Consensus Issue Investigation Workflow](ENGINEERING_PLAYBOOK.md#investigating-a-consensus-issue)
3. [Consensus Boundaries](CONSENSUS_BOUNDARIES.md)
4. [Testing Policy: Failure Handling](TESTING_POLICY.md#failure-handling)

Do not publish exploitable details before coordinated disclosure.

### Writing or superseding an engineering decision

Read:

1. [Decision Record Index](DECISIONS/README.md)
2. [ADR Workflow](ENGINEERING_PLAYBOOK.md#writing-an-architecture-decision-record)
3. the related policy and existing decision records.

Do not rewrite an accepted record to hide a reversal. Create a superseding
record.

## By Information Need

| Need | Document |
| --- | --- |
| Founder’s first-person statement | [Founder Vision](FOUNDER_VISION.md) |
| Project purpose and enduring compass | [Project Charter](PROJECT_CHARTER.md) |
| Long-term project mission | [Project Vision](PROJECT_VISION.md) |
| Engineering constitution | [Engineering Manifest](VISION_ENGINEERING_MANIFEST.md) |
| Reasons behind engineering practice | [Engineering Principles](ENGINEERING_PRINCIPLES.md) |
| Current repository truth | [Current Status](CURRENT_STATUS.md) |
| Ecosystem and component map | [Architecture Overview](ARCHITECTURE_OVERVIEW.md) |
| Implemented module detail | [Implemented Architecture](ARCHITECTURE.md) |
| Current HTTP routes | [HTTP API](API.md) |
| Current environment settings | [Configuration](CONFIGURATION.md) |
| Protected protocol areas | [Consensus Boundaries](CONSENSUS_BOUNDARIES.md) |
| Validation matrix and evidence | [Testing Policy](TESTING_POLICY.md) |
| Complete release lifecycle | [Release Process](RELEASE_PROCESS.md) |
| Rust and commit conventions | [Coding Standards](CODING_STANDARDS.md) |
| Recurring procedures | [Engineering Playbook](ENGINEERING_PLAYBOOK.md) |
| Project chronology | [Development History](DEVELOPMENT_HISTORY.md) |
| Completed, approved, planned, and visionary work | [Roadmap](ROADMAP.md) |
| Terminology | [Glossary](GLOSSARY.md) |
| Unused-code classifications | [Dead-Code Ledger](DEAD_CODE_LEDGER.md) |
| Historical warning measurements | [Quality Baseline](QUALITY_BASELINE.md) |
| License status | [License Decision Required](LICENSE_DECISION_REQUIRED.md) |
| Documentation audit result | [Documentation Consistency Audit](DOCUMENTATION_CONSISTENCY_AUDIT.md) |

## Engineering Decision Records

| Record | Decision |
| --- | --- |
| [0001](DECISIONS/0001_consensus_preservation.md) | Preserve consensus unless an explicit governed redesign is authorized |
| [0002](DECISIONS/0002_vpow_encoding.md) | Preserve historical VPoW encoding |
| [0003](DECISIONS/0003_release_identity.md) | Use immutable annotated release identity |
| [0004](DECISIONS/0004_deterministic_visionx_cache.md) | Keep VisionX cache behavior deterministic and transparent |
| [0005](DECISIONS/0005_cumulative_work_fork_choice.md) | Select competing valid chains by cumulative work |
| [0006](DECISIONS/0006_state_root_and_persistence_integrity.md) | Treat state root, persistence, and restart as one integrity boundary |
| [0007](DECISIONS/0007_deterministic_watchdog_testing.md) | Control watchdog test order without changing production peer behavior |
| [0008](DECISIONS/0008_developer_quality_baseline.md) | Perform Developer Readiness work through classified, narrow tranches |

## Phase I Boundary

Phase I established the Vision-Core engineering operating system:

- project purpose and engineering values;
- current-state and architecture references;
- consensus and validation policy;
- release and coding policy;
- operational playbooks;
- decision records;
- contributor navigation.

Phase I documentation is complete. Maintain it as living documentation when
code, policy, owner decisions, or release state changes.

The next sequence is:

1. complete the final promotion audit of the Developer Readiness stack;
2. push `dev/dead-code-cleanup-v104`;
3. allow GitHub Actions to validate the pushed branch;
4. review CI results;
5. fast-forward `main` only after explicit authorization;
6. create a fresh branch from updated `main`;
7. begin Configuration Hardening.

This sequence states current owner intent. It does not itself authorize a push,
promotion, branch creation, or Configuration Hardening implementation.

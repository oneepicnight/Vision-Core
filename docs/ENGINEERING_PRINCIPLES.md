# Engineering Principles

## Purpose

This document teaches future contributors how Vision engineering decisions are made. The rules exist because Vision-Core validates durable ownership and shared history. Reviewability, determinism, and traceable evidence are more important than the apparent speed of a large change.

These principles explain the reasons behind the operating system. Exact
procedures and minimum gates are defined once in the policy documents routed by
repository `AGENTS.md`.

## One Engineering Concern per Commit

Each commit should answer one review question. A focused commit is easier to reason about, validate, revert, promote, and audit years later. Documentation, formatting, tests, dependencies, behavior, and repository administration should be separated unless they are inseparable parts of the same concern.

## Consensus Changes Are Isolated

Anything that can alter canonical bytes, proof validation, transaction validity, fork choice, state transition, or state commitment receives its own design and change series. Consensus behavior must never be smuggled into cleanup or refactoring.

Isolation makes exact before-and-after behavior visible and prevents an unrelated edit from obscuring the protocol decision.

## Formatting Is Never Mixed with Logical Changes

Formatting rewrites produce large diffs without intended behavioral meaning. Mixing them with logic hides the meaningful lines, complicates blame, and weakens review. Repository-wide formatting therefore belongs in a dedicated commit, and logical work starts from that established baseline.

## Developer Documentation Comes Before Cleanup

An engineer cannot safely remove or consolidate code until ownership, supported APIs, historical compatibility, and persistence boundaries are understood. Documentation establishes those boundaries first.

Warnings and dead code are evidence to investigate, not automatic deletion instructions. A documented ledger lets the project distinguish obsolete code from public façades, test infrastructure, planned APIs, and historical protocol behavior.

## Evidence Always Outweighs Assumptions

Source, tests, canonical vectors, Git history, tags, release evidence, and reproducible commands outrank memory or comments. Comments are claims until confirmed.

When documents conflict with implementation, record the discrepancy and identify the controlling artifact. Never choose the interpretation that merely makes a task easier.

## Validation Is Required Before Promotion

Local success does not make a revision authoritative. A change is promoted only after the required focused, module, release, VisionX, persistence, restart, reorganization, and state-root gates have passed as applicable.

Evidence names the exact commit, command, toolchain, and outcome. Changing the candidate invalidates validation collected for the earlier revision.

## Every Change Has a Clearly Defined Scope

Before editing, state which behavior and files are in scope, which boundaries must remain unchanged, and what proves completion. Passing tests do not authorize adjacent improvements.

Explicit scope protects contributors from accidental protocol changes and gives reviewers a stable contract against which to assess the diff.

## Large Changes Are Divided into Reviewable Tranches

Broad initiatives are decomposed by risk and concern. Each tranche has its own authorization, commits, and validation. Low-risk documentation or test-infrastructure work precedes state-model or protocol-sensitive work.

Tranches create meaningful stopping points. If evidence reveals unexpected coupling, the project can freeze the affected tranche without discarding completed, independently valid work.

## Historical Consensus Behavior Is Preserved Unless Explicitly Redesigned

Published chain history and tags are durable evidence. Historical encodings, target semantics, proof behavior, and compatibility vectors are retained even when a unified modern implementation appears cleaner.

Changing that behavior requires a deliberate consensus proposal, activation and compatibility model, expanded validation, and explicit owner authorization. Cleanup is not redesign authority.

## Deterministic Testing Is Preferred

A reliable regression test exercises the same scenario on every run. Tests should control peer order, time, filesystem state, randomness, and concurrency where those variables matter to the assertion.

Arbitrary sleeps and execution-order assumptions weaken evidence. Repeated passes are useful only after the scenario itself is known to be deterministic.

## Repository History Should Remain Linear

Vision release promotion favors understandable ancestry and normal fast-forward or approved merge behavior. A linear history makes it easy to prove what entered a release, identify the prior authoritative state, and reproduce a candidate.

Work is organized so narrowly scoped commits can be reviewed and promoted in order.

## Force Pushes Are Avoided

Force pushes rewrite evidence other contributors or releases may already reference. They are avoided except under extraordinary, explicit repository-owner authorization with exact refs, preservation measures, and a documented recovery plan.

Public release tags are never moved. A correction receives a new commit, version, and tag.

## Consensus Work Receives Expanded Validation

Compilation and a focused unit test are not sufficient for consensus-sensitive changes. Relevant work receives historical-vector, full-release, VisionX, persistence, restart, reorganization, snapshot, state-root, clean-clone, and cross-node validation as applicable.

The test burden follows the possible consequence, not the number of edited lines.

## When Uncertainty Exists, Stop and Request Owner Guidance

Do not guess about licensing, public API promises, historical compatibility, security reporting, dormant protocol features, destructive Git actions, or the intended meaning of ambiguous state.

First exhaust read-only evidence. If a material decision remains, preserve the current state, state the ambiguity precisely, and request owner guidance.

## Engineering Decisions Are Documented

Important choices belong in version-controlled decision records. Future engineers should learn why a compatibility path exists, why a release tag is immutable, or why state cleanup was constrained without reconstructing months of conversation.

A reversal creates a superseding decision record rather than silently rewriting the earlier rationale.

## Supporting Principles

### Correctness before velocity

Prefer a small proved change to a broad elegant rewrite. Vision-Core’s mistakes can partition nodes or corrupt durable state.

### Fail visibly

Configuration, persistence, startup, and protocol errors should identify the failed invariant and relevant non-secret value. Silent fallback is legacy behavior to migrate deliberately.

### Separate protocol from product

Vision-Core is the blockchain authority. Desktop, wallet, exchange, marketplace, identity, and gaming systems consume its services without duplicating consensus.

### Honest maturity

Label capabilities as implemented, validated, experimental, dormant, planned, or visionary. A roadmap entry is not an existing protocol feature.

### Least authority

A local engineering task does not implicitly authorize pushes, tags, releases, branch deletion, external messages, or repository-setting changes.

### Reproducible handoffs

Every significant result should be recoverable from repository state and recorded evidence. The project must not depend on private chat history as institutional memory.

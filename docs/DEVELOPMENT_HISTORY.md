# Development History

## An Engineering History, Not a Changelog

Vision-Core evolved from an early blockchain implementation into a release-governed protocol repository with explicit consensus, networking, persistence, testing, and maintenance boundaries. This history explains the engineering decisions behind that evolution. Immutable commits and tags remain the final evidence.

## From Vision to Vision-Core

The original Vision work established the essential blockchain loop: blocks and transactions, chain state, persistence, proof-of-work validation, peer networking, mining, and tests for core invariants.

As the implementation matured, those responsibilities were consolidated into Vision-Core: the authoritative node responsible for canonical types, transaction execution, cumulative-work chain selection, networking, synchronization, storage, recovery, mining, and the application API. Product-facing work, including Vision Desktop, remained outside Core so a user interface could not become a second consensus implementation.

The pre-v1.0.3 public lineage culminated at `032a0f2`, “Preserve historical PoW target semantics.” That historical public state remains preserved through `archive/main-pre-v103-032a0f2`.

## Preserving Rather Than Rewriting Consensus

The project chose to preserve established chain interpretation instead of rewriting older protocol behavior to match a newer design. That decision accepted some implementation complexity in exchange for historical validity and node agreement.

The principle applies to:

- canonical encodings and exact byte order;
- historical target semantics;
- transaction and block identifiers;
- proof-of-work preimages;
- VisionX parameters and derivation;
- cumulative-work arithmetic;
- state transition and state-root rules;
- persisted chain interpretation.

Compatibility code is not dead merely because it is not the preferred modern path. A consensus rewrite would require a separately specified activation and compatibility plan, not a cleanup commit.

## VisionX Compatibility

VisionX developed through isolated stages:

- `94ec498` made the historical VPoW preimage encoder explicit;
- `067d227`, `6ad0e88`, and `d3421e7` established VisionX internals, dataset construction, and hashing;
- `55d2698` and `15357e8` added verification and mining interfaces;
- `0dd4c7c` integrated the historical preimage with mining;
- `52339bf` connected VisionX to block validation;
- `22b27db` moved production mining to VisionX;
- `2e71437` removed the temporary Blake3 compatibility placeholder;
- `34f5d38` made VisionX dataset caching deterministic.

Maintaining compatibility was essential because a proof implementation does more than find new blocks: it must validate established history identically on every node. Changing a preimage, target interpretation, cache result, or dataset derivation can split the network even if new blocks appear internally consistent.

Exact vectors and historical routing therefore became permanent consensus evidence.

## Transactions, State, and Recovery

Vision-Core’s state model grew through focused work on canonical transaction payloads, identifiers, Ed25519 signatures, stateless and stateful validation, deterministic state roots, side-chain reconstruction, atomic reorganization, and snapshot integrity.

Release milestones captured important state transitions:

- v1.0.0 established canonical mempool admission;
- v1.0.1 added coinbase reward crediting;
- v1.0.2 corrected reward/state-root consistency.

Networking and recovery then expanded beyond isolated unit behavior:

- compatible P2P handshakes;
- TCP catch-up synchronization;
- two-node and three-node harnesses;
- adversarial peer scenarios;
- restart persistence;
- snapshot reconciliation and tail replay;
- persisted canonical-height indexes;
- displaced-transaction recovery after reorganization;
- work-aware fork discovery;
- mining pause during higher-work recovery;
- peer recovery after restart.

`309debf` established the v1.0.3 cumulative-work rule: a fully validated branch with strictly greater work is eligible even when historical depth-policy constants would otherwise discourage the reorganization. `6a065df` ensured block acceptance always revalidated proof of work and became `vision-core-consensus-v1.0.3`.

## v1.0.4 as the Authoritative Baseline

The P2P watchdog recovery regression had a scheduling-sensitive test. Investigation found that production recovery logic was not the source delta required for release; the test needed a deterministic malicious-peer-then-valid-peer sequence.

Commit `b874d73cbdf60657334b62c867ed7f18b80a186b` changed only watchdog test infrastructure in `src/p2p/sync.rs`. Focused watchdog runs, the P2P module, full release tests, VisionX validation, repository topology, historical tags, and clean-clone behavior were audited.

The annotated tag `vision-core-consensus-v1.0.4` identifies that commit. `main` was advanced through a normal fast-forward with no history rewrite, and the earlier public state remained archived. v1.0.4 became the authoritative engineering baseline because it combined:

- the validated v1.0.3 consensus behavior;
- deterministic release-test infrastructure;
- immutable release identity;
- audited repository topology;
- evidence that the release delta introduced no runtime consensus or protocol change.

## Developer Readiness Initiative

After v1.0.4, the engineering objective changed from proving that the software worked to ensuring that a new contributor could maintain it safely. The project deliberately avoided general cleanup across the whole tree. Work was classified and divided into reviewable tranches.

### Developer Foundation

Purpose:

- establish developer entry points;
- document architecture, API, configuration, contribution, security, and consensus policy;
- pin the validated Rust toolchain;
- align package and runtime identity with v1.0.4;
- repair corrupted source documentation;
- add a baseline build and release-test workflow.

Problems solved:

- knowledge was scattered across source, history, and prior engineering sessions;
- toolchain expectations and release identity were not sufficiently explicit;
- contributors lacked a coherent starting path.

Risks intentionally avoided:

- no protocol redesign;
- no dependency modernization;
- no broad source cleanup;
- no claim that documentation created new API guarantees.

Validation:

- documentation and identity review;
- build and release-test checks;
- repository-scope verification;
- isolation of each engineering concern into its own commit.

### Formatting and Warning Baseline

Purpose:

- establish one stable formatting baseline;
- remove only unambiguous unused imports and test-only lint findings;
- modernize temporary-directory handling;
- measure compiler and Clippy debt instead of hiding it.

Problems solved:

- future logical diffs would otherwise be polluted by formatting churn;
- trivial warnings obscured the warnings that required ownership decisions;
- the project lacked a reproducible quality baseline.

Risks intentionally avoided:

- formatting was not mixed with logical behavior;
- warning removal did not authorize removal of public, consensus, protocol, persistence, or planned code;
- Clippy remained non-blocking while classified debt remained.

Validation:

- formatting checks;
- `cargo check`;
- focused tests for touched harness code;
- full release validation;
- recorded normal and test-target warning totals.

### Dead Code Classification

Purpose:

- inspect every warning and unused candidate;
- classify ownership, risk, and removal prerequisites;
- distinguish deletion candidates from historical compatibility and planned interfaces.

Problems solved:

- compiler output alone could not explain whether code belonged to tests, public API, dormant protocol behavior, persistence, or future work;
- broad cleanup would have produced an unreviewable and potentially consensus-sensitive diff.

Risks intentionally avoided:

- no code was removed during classification;
- historical VisionX compatibility remained protected;
- public façades, near-term APIs, uncertain ownership, and dormant protocol items were frozen;
- ChainState mempool fields were deferred pending a separate persistence and state-model audit.

Validation:

- repository-wide source and usage audit;
- review against architecture and Git history;
- a 48-entry [DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md) recording classifications and prerequisites.

### Dead Code Cleanup

Purpose:

- remove only owner-approved, evidenced candidates;
- restore a missing test annotation;
- prove that narrowly scoped state-model cleanup preserved behavior.

Tranche 3A used separate commits to remove an obsolete bootstrap test helper, remove `NodeHarness::api_addr`, and restore `#[test]` on `cumulative_work_single_block`.

Tranche 3B used one isolated commit to remove the unread, non-persisted `ChainState` fields `mempool_critical`, `mempool_bulk`, `mempool_ts`, and `mempool_height`, plus only directly consequent imports.

Problems solved:

- obsolete test infrastructure no longer obscures active harness behavior;
- the cumulative-work single-block test again participates in the suite;
- the in-memory state model no longer advertises four fields proven to have no runtime or persistence ownership.

Risks intentionally avoided:

- no dormant consensus or protocol feature removal;
- no public API removal;
- no historical VisionX compatibility changes;
- no database format change;
- no unrelated cleanup in the ChainState commit.

Validation:

- `cargo check`;
- focused watchdog validation;
- directly affected tests;
- full release suite;
- VisionX suite;
- persistence and restart checks;
- reorganization tests;
- snapshot and state-root checks;
- `git diff --check`.

## From Passing Tests to Maintainable Engineering

Passing tests established a necessary baseline, but maintainability required more:

- a documented authority hierarchy;
- explicit consensus and persistence boundaries;
- immutable release identity;
- deterministic regression scenarios;
- warning and dead-code classification;
- narrowly scoped history;
- evidence-driven promotion;
- durable decision records;
- clear separation between implemented behavior and future vision.

The Project Intelligence Layer completes that transition by making project intent, architecture, history, current state, policy, and unresolved decisions available in the repository rather than in private conversation.

## Current Direction

The current local developer line contains the completed modernization stack but
is not yet the public authoritative release line. The next approved engineering
task is Configuration Hardening, which changes runtime startup behavior and
therefore begins only after the Developer Readiness stack is reviewed, pushed,
and promoted.

After that foundation, engineering emphasis shifts from repository modernization toward controlled protocol evolution, operational maturity, supported client interfaces, and new functionality. Consensus, persistence, and historical compatibility continue to require explicit design rather than incidental cleanup.

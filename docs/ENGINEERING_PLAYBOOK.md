# Engineering Playbook

## Purpose

This playbook turns Vision-Core policy into repeatable workflows for human and
automated contributors. It does not grant authorization and does not replace
[CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md),
[TESTING_POLICY.md](TESTING_POLICY.md),
[RELEASE_PROCESS.md](RELEASE_PROCESS.md), or
[CODING_STANDARDS.md](CODING_STANDARDS.md).

## Universal Intake

Before investigating or editing:

1. Read repository `AGENTS.md`.
2. Read [CURRENT_STATUS.md](CURRENT_STATUS.md).
3. Record:

   ```powershell
   git status --short --branch
   git rev-parse HEAD
   git describe --tags --always
   ```

4. Identify and preserve pre-existing changes.
5. Restate the outcome, authorized mutations, forbidden actions, and required
   validation.
6. Classify consensus, protocol, persistence, configuration, API, operational,
   test, and documentation impact.
7. Read relevant source, tests, history, policies, and decision records.

If material authority or ownership remains unclear, stop at the boundary and
request owner guidance.

## Investigating a Consensus Issue

1. Preserve the exact failing block, transaction, height, hash, state root,
   proof input, logs, and node version where available.
2. Reproduce without modifying golden vectors or historical fixtures.
3. Identify the first divergent boundary:
   - canonical bytes;
   - proof or target calculation;
   - transaction validation;
   - state transition;
   - fork choice;
   - persistence or restart;
   - network import path.
4. Compare behavior with the last authoritative release.
5. Inspect historical encoders and version or height routing.
6. Determine whether new blocks, historical blocks, or both are affected.
7. Design the smallest deterministic regression test.
8. Document network-split, state-corruption, and compatibility consequences.
9. Stop before changing behavior unless explicit consensus authorization
   exists.

The investigation output states reproduction, affected invariant, earliest
divergence, compatibility assessment, proposed validation, and owner decisions
required.

## Classifying a Proposed Change

Ask in order:

1. Can it change a hashed, signed, serialized, or persisted byte?
2. Can it change block or transaction acceptance?
3. Can it change proof verification, difficulty, or cumulative work?
4. Can it change state after connection, reorg, restart, or snapshot restore?
5. Can it change peer or protocol compatibility?
6. Can it change startup, API, or operator-visible behavior?
7. Is it solely documentation, formatting, or test infrastructure?

Classify by the highest-risk “yes.” If evidence is incomplete, use the safer
class. Select validation from [TESTING_POLICY.md](TESTING_POLICY.md).

## Deciding Commit Boundaries

A change belongs in a separate commit when it has a distinct:

- review question;
- rollback decision;
- authorization boundary;
- validation profile;
- compatibility effect;
- subsystem owner.

Always separate:

- formatting from logic;
- dependencies from behavior;
- consensus from cleanup;
- persistence formats from in-memory refactoring;
- characterization tests from behavior changes;
- release administration from source development;
- unrelated documentation corrections.

If a subject requires “and” to join independent outcomes, split the commit.

## Reviewing a Pull Request

1. Verify base, head, ancestry, and complete diff.
2. Compare the stated scope with changed files and behavior.
3. Identify protected boundaries using
   [CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md).
4. Review canonical bytes, arithmetic, ordering, mutation atomicity, error
   categories, and persistence effects.
5. Confirm tests prove the claim and would detect the original defect.
6. Confirm validation evidence names the reviewed commit.
7. Check documentation, migration notes, and decision records.
8. Check commit separation and unexpected generated or formatting changes.
9. Record findings by severity and exact file or location.
10. Do not approve while consensus, compatibility, data-loss, or authorization
    questions remain unresolved.

## Performing a Dead-Code Audit

1. Capture the compiler and Clippy baseline.
2. Enumerate candidates without deleting them.
3. Search runtime, tests, re-exports, serialization, configuration, and
   feature-gated uses.
4. Inspect Git history and decision records for ownership.
5. Classify each item:
   - low-risk private candidate;
   - test infrastructure;
   - supported or undecided public façade;
   - near-term planned API;
   - uncertain ownership;
   - persistence or state-model sensitive;
   - protocol or consensus sensitive;
   - historical compatibility.
6. Record evidence and prerequisites in
   [DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md).
7. Obtain authorization for named candidates.
8. Remove each concern in a narrow commit.
9. Record warning-count transitions and run risk-proportional validation.

Compiler output is discovery evidence, not deletion authority.

## Writing an Architecture Decision Record

1. Choose the next unused four-digit number.
2. Use a descriptive snake-case filename under `docs/DECISIONS`.
3. State status and scope.
4. Describe repository-supported context and constraints.
5. State one decision precisely.
6. Record consequences, including costs and compatibility effects.
7. Link source, tests, commits, policy, or release evidence.
8. Mark unsupported choices **Owner Decision Required**.
9. Add the record to `docs/DECISIONS/README.md`.

Do not rewrite an accepted record to conceal a reversal. Create a superseding
record.

## Preparing a Release

Follow [RELEASE_PROCESS.md](RELEASE_PROCESS.md).

1. Assemble only approved commits on the integration branch.
2. Audit ancestry and the diff from the prior release.
3. Freeze the candidate commit and tree.
4. Align version identity and release notes.
5. Run the complete validation matrix.
6. Audit local and remote refs and historical tags.
7. Prepare an approval package naming exact actions.
8. Wait for explicit owner authorization.
9. Promote using the authorized normal Git operation.
10. Create and push only the authorized annotated tag.
11. Publish approved notes.
12. Perform post-release verification.

## Conducting Post-Release Validation

1. Resolve the remote tag object and peeled commit.
2. Verify the authoritative branch and required archival refs.
3. Confirm historical tags remain unchanged.
4. Check out or clone the tag into a clean workspace.
5. Verify version identity and the dependency lockfile.
6. Run the authorized smoke or release suite.
7. Compare results with candidate evidence.
8. Verify notes and artifacts refer to the same commit.
9. Record final topology and deviations.

Branch retirement remains a separate authorized action.

## Recovering from a Failed Release Candidate

### Before publication

- preserve the failed commit and evidence;
- do not tag it;
- classify product, test, environment, or topology failure;
- correct through a new isolated commit;
- define a new candidate;
- rerun invalidated validation.

### After branch promotion

- stop before tag creation;
- preserve prior and promoted refs;
- assess whether the branch was externally consumed;
- request an owner decision on forward correction or authorized ref
  restoration.

### After tag publication

- never move the tag;
- publish an advisory if required;
- assess consensus, persistence, protocol, and security impact;
- prepare a new version through the full release process;
- document operator recovery or migration.

Emergency chain and database recovery is **Owner Decision Required** unless an
accepted incident plan governs the event.

## Evaluating Protocol-Change Safety

1. State the exact old rule and proposed new rule.
2. Identify affected versions, heights, networks, messages, and stored data.
3. Define activation and coexistence behavior.
4. Determine old-node and new-node interaction before, during, and after
   activation.
5. Produce canonical byte and state vectors.
6. Analyze historical-block validation and reorganization.
7. Analyze rollback, downgrade, and database compatibility.
8. Identify miner, wallet, Desktop, exchange, and operator effects.
9. Define monitoring and failure signals.
10. Obtain owner approval before implementation.
11. Implement in isolated commits.
12. Run expanded consensus and protocol validation.

If activation, governance, or rollback is unsupported by repository evidence,
mark it **Owner Decision Required**.

## Configuration Hardening Workflow

1. Inventory each setting, source, default, parser, consumer, and documentation
   claim.
2. Record current valid, invalid, missing, and fallback behavior.
3. Decide intended behavior within the approved scope.
4. Add deterministic startup tests before changing parsing.
5. Implement one behavior class per commit.
6. Produce actionable, non-secret errors.
7. Prevent cross-network database reuse.
8. Document migration from formerly accepted fallback behavior.
9. Run configuration, startup, watchdog, VisionX, release, and CI validation.

Configuration work does not authorize consensus constants, genesis identity,
protocol versions, or persistence-format changes.

## Task Handoff

Every material handoff states:

- starting and final commit or uncommitted tree;
- branch and worktree state;
- files and behavior changed;
- consensus, protocol, persistence, API, and runtime impact;
- commands and exact results;
- warning and ignored-test changes;
- unresolved owner decisions;
- remote refs or external systems changed;
- safe next action.

The handoff must be understandable without the originating conversation.

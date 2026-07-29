# Vision-Core Engineering Operating Contract

This file is the mandatory entry point for every human or automated engineering
session in this repository. It applies to the entire repository.

## Required Reading

Read these documents before editing:

1. [PROJECT_CHARTER.md](docs/PROJECT_CHARTER.md)
2. [PROJECT_VISION.md](docs/PROJECT_VISION.md)
3. [CURRENT_STATUS.md](docs/CURRENT_STATUS.md)
4. [ARCHITECTURE_OVERVIEW.md](docs/ARCHITECTURE_OVERVIEW.md)
5. [VISION_ENGINEERING_MANIFEST.md](docs/VISION_ENGINEERING_MANIFEST.md)
6. [ENGINEERING_PRINCIPLES.md](docs/ENGINEERING_PRINCIPLES.md)
7. [CONSENSUS_BOUNDARIES.md](docs/CONSENSUS_BOUNDARIES.md)
8. [TESTING_POLICY.md](docs/TESTING_POLICY.md)
9. [CODING_STANDARDS.md](docs/CODING_STANDARDS.md)
10. [ENGINEERING_PLAYBOOK.md](docs/ENGINEERING_PLAYBOOK.md)
11. the relevant accepted record under
    [docs/DECISIONS](docs/DECISIONS/README.md).

For release work, also read [RELEASE_PROCESS.md](docs/RELEASE_PROCESS.md). For
configuration, API, security, licensing, quality-baseline, or dead-code work,
read the corresponding dedicated document linked from
[CURRENT_STATUS.md](docs/CURRENT_STATUS.md).

Reading is mandatory before implementation is selected.

## Policy Ownership

Each operational rule has one controlling document:

| Subject | Controlling document |
| --- | --- |
| Current release, branch, commit, tests, warnings, and active work | `docs/CURRENT_STATUS.md` |
| Consensus and protocol classification and authorization | `docs/CONSENSUS_BOUNDARIES.md` |
| Validation requirements and evidence | `docs/TESTING_POLICY.md` |
| Release promotion, tagging, publication, and failed candidates | `docs/RELEASE_PROCESS.md` |
| Rust, module, error, documentation, and commit conventions | `docs/CODING_STANDARDS.md` |
| Recurring engineering workflows | `docs/ENGINEERING_PLAYBOOK.md` |
| Unused-code disposition | `docs/DEAD_CODE_LEDGER.md` |
| Accepted architectural rationale | `docs/DECISIONS/` |

The charter, vision, manifest, principles, history, roadmap, and architecture
overview provide purpose, rationale, chronology, direction, and system context.
They do not override the controlling policies above.

If documentation conflicts with executable source, locked vectors, or Git
objects, stop and report the conflict. Prefer repository evidence and correct
documentation only when the evidence is conclusive.

## Establish Repository State

Before editing, record:

```powershell
git status --short --branch
git branch --show-current
git rev-parse HEAD
git describe --tags --always
```

Distinguish `HEAD`, fetched `origin/main`, and the latest release tag. They are
not interchangeable.

Stop if the worktree contains changes that are not understood. Do not clean,
reset, stash, overwrite, or absorb another contributor’s work without explicit
authorization.

## Task Execution

For every task:

1. identify the requested outcome, exact authorization, and forbidden actions;
2. classify the change under `docs/CONSENSUS_BOUNDARIES.md`;
3. inspect relevant source, tests, Git history, policies, and decisions;
4. define intended files, commit boundaries, and validation;
5. implement only the authorized scope;
6. review the complete diff;
7. validate under `docs/TESTING_POLICY.md`;
8. update maintained documentation when project truth changes;
9. hand off exact evidence and unresolved owner decisions.

Use `docs/ENGINEERING_PLAYBOOK.md` for recurring workflows. Passing validation
does not broaden the authorized scope.

## Repository and History Safety

- Preserve published commits, tags, and release history.
- Do not push, merge, tag, delete branches, rewrite history, force-push, or
  change repository settings unless the task explicitly authorizes that exact
  operation.
- Do not remove code solely because a compiler reports it unused.
- Do not update golden vectors merely to match a changed implementation.
- Do not describe changed consensus, serialized bytes, state, persistence, or
  compatibility as cleanup.
- Do not present planned or visionary capability as implemented.

Detailed consensus classifications belong to
`docs/CONSENSUS_BOUNDARIES.md`; this file intentionally does not duplicate
them.

## Stop Conditions

Stop and request owner guidance when:

- protected behavior may change without explicit authorization;
- activation, migration, rollback, security-intake, licensing, or public API
  policy is not defined by repository evidence;
- overlapping worktree changes cannot be attributed safely;
- validation reveals an unexplained deterministic failure;
- completion requires an unauthorized remote or destructive Git operation;
- a cleanup candidate remains frozen in `docs/DEAD_CODE_LEDGER.md`.

Mark unsupported policy choices **Owner Decision Required**. Do not guess.

## Validation

Select and run the authoritative minimum gates in
[TESTING_POLICY.md](docs/TESTING_POLICY.md). Read current expected counts and
warning baselines from [CURRENT_STATUS.md](docs/CURRENT_STATUS.md); do not copy
historical totals forward.

Use `--offline` only when the pinned toolchain and every locked dependency
required by the selected command are already cached.

## Required Handoff

Use the handoff format in `docs/ENGINEERING_PLAYBOOK.md`. At minimum, report:

- starting and final commit or uncommitted tree;
- branch and worktree state;
- exact files and behavior changed;
- consensus, protocol, persistence, API, and runtime impact;
- exact validation commands and results;
- warnings, ignored tests, limitations, and owner decisions;
- remote refs or external systems changed.

Never imply that uncommitted or unpushed work has been published.

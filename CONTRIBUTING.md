# Contributing to Vision-Core

Read repository [AGENTS.md](AGENTS.md) before investigating or changing
Vision-Core. It defines the mandatory reading order, policy ownership,
repository-state checks, stop conditions, and handoff requirements.

## Preparing Work

- Begin from the owner-approved base and verify its full commit.
- Use a descriptive branch such as `fix/<focused-defect>`,
  `test/<focused-validation>`, or `docs/<documentation-scope>`.
- Define the intended behavior, explicit exclusions, and authorization before
  editing.
- Preserve unrelated worktree changes.
- Use one reviewable engineering concern per commit.

Commit and Rust conventions are defined in
[CODING_STANDARDS.md](docs/CODING_STANDARDS.md).

## Classification and Validation

Classify every change under
[CONSENSUS_BOUNDARIES.md](docs/CONSENSUS_BOUNDARIES.md). When uncertainty
remains, use the consensus-sensitive path and request owner guidance.

Select focused and broader validation from
[TESTING_POLICY.md](docs/TESTING_POLICY.md). Compare warning results with the
historical [Tranche 2 quality baseline](docs/QUALITY_BASELINE.md) and the
current baseline in [CURRENT_STATUS.md](docs/CURRENT_STATUS.md).

Before removing an unused item, consult
[DEAD_CODE_LEDGER.md](docs/DEAD_CODE_LEDGER.md). An unused-code diagnostic is
not deletion authorization.

## Pull Requests

A pull request or equivalent review package states:

- baseline and candidate commits;
- files and behavior in scope;
- explicitly excluded behavior;
- consensus, protocol, persistence, API, and runtime impact;
- exact validation commands and results;
- warning and ignored-test changes;
- migrations, compatibility effects, and owner decisions;
- documentation updated.

Do not add unrelated cleanup discovered during review.

## Repository Governance

Release promotion, tagging, publication, failed-candidate handling, and branch
retirement follow [RELEASE_PROCESS.md](docs/RELEASE_PROCESS.md). A source
change does not authorize any remote or repository-governance operation.

Security reports follow [SECURITY.md](SECURITY.md). The repository does not yet
contain an authoritative license; see
[LICENSE_DECISION_REQUIRED.md](docs/LICENSE_DECISION_REQUIRED.md).

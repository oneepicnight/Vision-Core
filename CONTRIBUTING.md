# Contributing to Vision-Core

## Branches

Use a descriptive branch from the current authoritative `origin/main`, for
example:

- `dev/<developer-foundation>`
- `fix/<focused-defect>`
- `test/<focused-validation>`
- `docs/<documentation-scope>`

Do not rewrite published history, move release tags, or force-push shared
release branches.

## Commit isolation

Each commit should have one reviewable concern. Keep formatting, dependency
updates, documentation, tests, and behavior changes separate. Do not mix
opportunistic cleanup into a focused fix.

## Build and test

```powershell
cargo check --all-targets --locked
cargo test --release --locked -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --locked
```

Use `--offline` only after the locked dependencies and selected toolchain are
available locally.

The release suite is intentionally single-threaded. A focused change should
also run its narrowest applicable test target before the full suite.

## Formatting and lint

New work should be formatted and should not introduce new warnings. The v1.0.4
baseline contains known formatting and Clippy debt, so compare results against
the documented baseline rather than hiding failures. Repository-wide formatting
belongs in its own tranche and commit.

## Consensus-sensitive review

Read `docs/CONSENSUS_CHANGE_POLICY.md` before modifying block encoding, PoW,
VisionX, genesis, transaction execution, chain acceptance, state roots,
persistence compatibility, or P2P wire types.

Such changes require:

- an isolated commit and branch;
- characterization or golden-vector tests;
- serialization and historical-compatibility review;
- focused validation;
- the complete single-threaded release suite;
- cross-machine evidence where relevant.

If uncertain whether a change is consensus-sensitive, classify it as sensitive
until reviewed.

## Pull requests

State:

- the baseline commit;
- files and behavior in scope;
- explicitly excluded behavior;
- tests run and exact results;
- protocol, consensus, storage, and runtime impact;
- known warnings or follow-up work.

Do not merge unrelated cleanup merely because it was discovered during review.

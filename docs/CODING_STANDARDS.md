# Coding Standards

## Rust toolchain and formatting

Use the exact toolchain declared in `rust-toolchain.toml`. It is the currently
validated toolchain, not a claimed minimum supported Rust version.

Before handoff, run the applicable formatting, check, focused, and broader
gates selected from [TESTING_POLICY.md](TESTING_POLICY.md). That document is the
single authority for validation commands.

Run `cargo fmt --all` only as an intentional formatting operation. Never mix a
repository-wide formatting change with behavioral work.

## Naming and structure

- Use domain names that identify the rule being implemented.
- Keep canonical encoding and identity functions explicit; avoid ambiguous
  helpers such as generic `hash()` where multiple identities exist.
- Keep protocol, consensus, policy, and application constants distinguishable.
- Prefer small functions with typed results at validation boundaries.
- Keep test helpers inside `#[cfg(test)]` modules unless runtime code requires
  them.
- Do not create duplicate sources of truth for versions or constants.

## Module organization

- Organize modules by domain ownership, not incidental call order.
- Keep canonical types and encodings close to their invariants and vectors.
- Keep block acceptance common to peer, synchronization, orphan, and
  local-mining blocks.
- Keep HTTP and other application adapters outside consensus logic.
- Do not duplicate validation in API, networking, or mining layers.
- Centralize persistence keys and encoding helpers under explicit names.
- Keep test harnesses under `#[cfg(test)]` or dedicated test modules.
- Prefer private visibility; expand it only for a documented caller or
  supported interface.
- Do not treat existing `pub` visibility as proof of a supported library API.

New modules should have one clear owner and should not create circular
dependencies among consensus, persistence, networking, and application layers.

## Errors

- Return structured errors where callers make decisions.
- Include the failed invariant and actionable context.
- Never include private keys, signatures beyond necessary diagnostics, or
  sensitive environment contents in logs.
- Do not convert validation failures into panics.
- Do not use `unwrap` or `expect` in runtime paths where malformed peer, API,
  storage, or configuration input can reach the code.
- Tests may use `unwrap` when failure should abort the test and the context is
  obvious.
- Preserve structured categories when callers use errors for peer, recovery,
  retry, or state decisions.
- Add context at subsystem boundaries without erasing the original cause.
- Do not use error-message text as machine-readable control flow.
- Configuration errors should name the setting and rejected non-secret value.
- Persistence errors should name the operation and key category without
  dumping sensitive or unbounded data.

## Logging

Use `tracing` consistently:

- `error`: service cannot preserve its contract or must abort;
- `warn`: degraded operation, rejected persisted state, or recoverable
  incompatibility;
- `info`: lifecycle and major canonical-state transitions;
- `debug`: diagnostic decisions and detailed recovery flow;
- `trace`: high-volume internal detail.

Include stable fields or unambiguous labels for heights, hashes, peer addresses,
and modes. Truncate hashes only for human logs, never for comparisons or stored
identity.

## Consensus-sensitive code

For canonical bytes:

- state byte order and integer endianness explicitly;
- use fixed-width types;
- reject malformed data rather than normalizing it;
- preserve exact vector tests;
- avoid unordered collections in committed output;
- do not use floating-point arithmetic for consensus decisions;
- document version/activation implications.

For state transitions:

- validate on temporary state before committing;
- avoid partial mutation on rejection;
- compute and verify state roots deterministically;
- route peer and locally mined blocks through the same acceptance path;
- persist canonical transitions atomically where the storage API permits.

## APIs and configuration

- Treat response shapes and error codes as contracts once documented stable.
- Until stabilization, document inconsistencies instead of hiding them.
- Parse configuration once, validate explicitly, and report the setting name.
- Do not introduce new silent fallback.
- Do not add a setting until its runtime consumer exists.

## Dependencies

Dependency changes require a separate commit and rationale. Preserve
`Cargo.lock`. Evaluate:

- consensus determinism;
- serialization compatibility;
- cryptographic behavior;
- platform support;
- license and maintenance status;
- transitive changes.

## Warnings and dead code

New or modified code must not add warnings. Do not apply blanket `allow`
attributes to improve a count. Consult `docs/DEAD_CODE_LEDGER.md` before
removing unused symbols, and require an explicit design decision for public,
dormant consensus, protocol, or historical compatibility items.

## Comments and documentation

Explain invariants, encoding, compatibility, and non-obvious reasons. Do not
narrate syntax. Update `CURRENT_STATUS.md` and a decision record when a change
alters the maintained project truth.

Protected or supported interfaces document:

- purpose;
- input and output semantics;
- failure behavior;
- consensus, protocol, persistence, or application classification;
- compatibility or version constraints;
- examples only when readily verifiable.

Use a decision record for choices future maintainers might otherwise reverse
without understanding the consequence.

## Test code

- Name tests for observable behavior, not implementation details.
- A regression test must fail for the original defect for the stated reason.
- Keep fixtures minimal and deterministic.
- Control time, peer order, randomness, and filesystem state where relevant.
- Do not weaken an assertion until the intended contract is reviewed.
- Do not rely on global test order or residue from another test.
- Report intentionally ignored tests in the maintained baseline.

## Commit style

Each commit contains one engineering concern and should be independently
reviewable and revertible.

Commit subjects:

- use the imperative mood;
- describe the outcome rather than the editing activity;
- identify the affected subsystem;
- do not claim consensus neutrality without evidence;
- avoid vague subjects such as “cleanup” or “fix warnings.”

Examples:

```text
Restore single-block cumulative-work test
Remove unused multi-node API address field
Document persistence migration boundary
Reject invalid mining address at startup
```

Use the body when rationale, compatibility, validation, or a non-obvious
constraint cannot be understood from the diff. Keep consensus, protocol,
persistence, dependencies, formatting, and governance in separate commits.

Do not amend, reorder, squash, or force-push published work without explicit
authorization. Release tags are immutable.

## Review readiness

Before handoff:

- inspect `git diff` and `git diff --check`;
- confirm only authorized files changed;
- run validation required by [TESTING_POLICY.md](TESTING_POLICY.md);
- update affected documentation and decision records;
- report warning-count changes;
- state consensus, protocol, persistence, API, and runtime impact;
- identify unresolved owner decisions.

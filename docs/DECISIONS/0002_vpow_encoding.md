# 0002: Preserve Historical VPoW Encoding

- Status: Accepted
- Scope: Proof-of-work compatibility

## Context

Vision-Core proof validation includes historical VPoW preimage behavior. Replacing historical encoding with a modern canonical helper can produce different proof inputs and invalidate established history even when the replacement looks internally consistent.

## Decision

Keep the historical VPoW encoder explicit and use it for the chain history it governs. Treat field selection, ordering, byte widths, byte order, and height/version routing as consensus rules.

Do not remove or normalize the compatibility path without a new consensus decision that defines activation, migration, and historical validation behavior.

## Consequences

- Historical encoding receives dedicated vectors and regression tests.
- Modern and historical encoders may coexist.
- Shared-helper refactors must compare exact bytes, not semantic field values.
- Proof-of-work optimization must preserve the selected encoding path.

## Evidence

Repository history includes the preservation and integration work represented by commits `94ec498` and `0dd4c7c`, with related target-semantics preservation at `032a0f2`. See [CONSENSUS_BOUNDARIES.md](../CONSENSUS_BOUNDARIES.md).

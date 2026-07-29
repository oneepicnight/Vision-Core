# 0001: Consensus Preservation

- Status: Accepted
- Scope: Consensus and protocol maintenance

## Context

Vision-Core has an established chain history and public release lineage. Seemingly local changes to encoding, arithmetic, validation ordering, caches, persistence, or iteration can cause honest nodes to disagree. Cleanup value does not justify an implicit network rule change.

## Decision

Preserve existing consensus behavior unless a separately designed, reviewed, activated, and released consensus change is explicitly authorized.

Any unresolved change that can affect canonical bytes, proof validation, fork choice, state transition, state commitment, or historical interpretation is classified as consensus-sensitive. Consensus changes must not be hidden inside refactors, warning cleanup, dependency updates, or performance work.

## Consequences

- Historical blocks must continue to validate under their governing rules.
- Refactors require equivalence evidence, not only compilation.
- Compatibility paths may remain even when a unified implementation appears cleaner.
- Consensus work requires explicit activation and compatibility design.
- Non-consensus cleanup must demonstrate that emitted bytes and state outcomes are unchanged.

## Evidence

This decision is reflected in [CONSENSUS_CHANGE_POLICY.md](../CONSENSUS_CHANGE_POLICY.md), [CONSENSUS_BOUNDARIES.md](../CONSENSUS_BOUNDARIES.md), and the release history documented in [DEVELOPMENT_HISTORY.md](../DEVELOPMENT_HISTORY.md).

# 0006: State-Root and Persistence Integrity

- Status: Accepted
- Scope: Chain state, storage, restart, and snapshots

## Context

An in-memory state model is insufficient if restart, rollback, or snapshot restoration derives a different state from the same chain. Removing apparently unused state fields can also expose an undocumented persistence invariant.

## Decision

Treat committed state, its deterministic state root, and its persisted reconstruction as one integrity boundary. State-model changes require restart, reorganization, and state-root evidence. Snapshot data is verified before becoming authoritative.

Do not remove ChainState fields solely because the compiler reports no reads. First audit serialization, recovery, rollback, snapshot, and downstream ownership.

## Consequences

- Connect/disconnect operations must be reversible where reorganization requires it.
- Restart must reproduce the same selected tip, UTXO state, cumulative work, and state root.
- Snapshot import checks network, chain, and commitment metadata.
- Persistence changes require explicit compatibility or migration handling.
- State-model cleanup remains isolated from unrelated polish.

## Evidence

The repository’s chain-state and persistence hardening history is summarized in [DEVELOPMENT_HISTORY.md](../DEVELOPMENT_HISTORY.md). Validation requirements are in [TESTING_POLICY.md](../TESTING_POLICY.md).

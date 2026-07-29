# 0005: Cumulative-Work Fork Choice

- Status: Accepted
- Scope: Chain selection and reorganization

## Context

Height alone does not measure the proof-of-work represented by a chain when block difficulties differ. Selecting a taller but lower-work branch would violate proof-of-work chain-selection semantics.

## Decision

Compare competing valid Vision-Core chains by cumulative work. Maintain and verify cumulative-work metadata across block connection, persistence, restart, and reorganization.

Tie behavior and arithmetic must be deterministic. Overflow, malformed targets, and missing ancestor data fail safely.

## Consequences

- Chain-state tests include branches with unequal height and work.
- Persisted cumulative work is validated or reconstructable.
- Reorganization tests assert both the selected tip and resulting state.
- Fork-choice changes are consensus changes.

## Evidence

Deep-reorganization cumulative-work hardening is represented by commit `309debf`. See [ARCHITECTURE_OVERVIEW.md](../ARCHITECTURE_OVERVIEW.md) and [CONSENSUS_BOUNDARIES.md](../CONSENSUS_BOUNDARIES.md).

# 0008: Developer Quality Baseline

- Status: Accepted
- Scope: Maintenance and developer readiness

## Context

Validated protocol software can still be difficult to maintain when warnings, dead code, duplicated utilities, inconsistent errors, stale configuration, and undocumented ownership accumulate. Broad cleanup, however, obscures review and can cross consensus or persistence boundaries.

## Decision

Improve developer readiness through classified, narrow tranches. Each removal or cleanup is justified by repository evidence, isolated by concern, and validated according to risk.

Begin with test-infrastructure and clearly private candidates. Treat public façades, dormant protocol features, historical VisionX compatibility, and ChainState/persistence fields as design or state-model decisions rather than compiler-driven deletion candidates.

## Consequences

- The dead-code ledger records disposition and evidence.
- Low-risk items use narrowly scoped changes.
- Core state-model cleanup receives restart, reorganization, and state-root validation.
- Remaining uncertain items stay frozen until ownership or API decisions exist.
- Passing validation does not broaden an approved cleanup scope.

## Evidence

The classification record is [DEAD_CODE_LEDGER.md](../DEAD_CODE_LEDGER.md). Current developer-line state and validation counts are recorded in [CURRENT_STATUS.md](../CURRENT_STATUS.md).

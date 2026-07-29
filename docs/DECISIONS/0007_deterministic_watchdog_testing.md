# 0007: Deterministic Watchdog Testing

- Status: Accepted
- Scope: P2P synchronization test infrastructure

## Context

The watchdog recovery regression depends on observing failure with a malicious peer followed by recovery with a valid peer. Uncontrolled peer ordering made the test scheduling-dependent even though production recovery behavior was unchanged.

## Decision

Control peer order inside the test infrastructure so the regression exercises the intended sequence deterministically. Do not change production peer selection or synchronization protocol behavior to satisfy the test.

## Consequences

- The focused test proves the malicious-then-valid recovery path.
- Repeated validation is meaningful because the scenario is fixed.
- Test-only controls remain clearly separated from production logic.
- Future watchdog work must preserve deterministic observability without arbitrary timing assumptions.

## Evidence

Commit `b874d73cbdf60657334b62c867ed7f18b80a186b` contains the deterministic test correction and is the commit identified by `vision-core-consensus-v1.0.4`.

# 0004: Deterministic VisionX Cache

- Status: Accepted
- Scope: VisionX proof-of-work implementation

## Context

VisionX uses derived data and caching for practical validation performance. A cache can become consensus-dangerous if process history, concurrency, eviction, or platform behavior changes the value returned for the same consensus input.

## Decision

VisionX cache behavior must be a transparent performance optimization. Cache keys and derived values are deterministic functions of explicit protocol inputs. A cache hit and a clean recomputation must produce identical validation results.

Concurrency control must prevent partial publication and cross-key contamination. Tests must cover reuse, clean initialization, and concurrent access.

## Consequences

- Cache state is not consensus state.
- Eviction may affect performance but not proof results.
- Cache-format changes require focused compatibility and determinism testing.
- Hidden environment, clock, and unordered-iteration inputs are prohibited.

## Evidence

The deterministic dataset-cache implementation is represented in repository history by commit `34f5d38`. VisionX validation remains a distinct release gate under [TESTING_POLICY.md](../TESTING_POLICY.md).

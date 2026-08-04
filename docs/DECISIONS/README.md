# Engineering Decision Records

Decision records capture durable choices that future maintainers must understand before changing Vision-Core. They explain the reason and consequences of a decision; they do not override source code, immutable release artifacts, or the controlling policies.

## Status Vocabulary

- **Accepted**: the decision governs current work.
- **Superseded**: a later record replaces the decision.
- **Proposed**: review is incomplete and the record is not authoritative.
- **Deprecated**: retained for history but should not guide new work.

## Index

1. [Consensus Preservation](0001_consensus_preservation.md)
2. [VPoW Encoding](0002_vpow_encoding.md)
3. [Release Identity](0003_release_identity.md)
4. [Deterministic VisionX Cache](0004_deterministic_visionx_cache.md)
5. [Cumulative-Work Fork Choice](0005_cumulative_work_fork_choice.md)
6. [State-Root and Persistence Integrity](0006_state_root_and_persistence_integrity.md)
7. [Deterministic Watchdog Testing](0007_deterministic_watchdog_testing.md)
8. [Developer Quality Baseline](0008_developer_quality_baseline.md)
9. [Readiness and Health-State Policy](0009_readiness_health_state_policy.md)
10. [Synchronization Control-Message Interleaving](0010_sync_control_message_interleaving.md)

New records use the next four-digit number and a descriptive snake-case filename. Accepted records are not rewritten to conceal earlier reasoning; material reversals require a superseding record.

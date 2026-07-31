# ADR-0009: Readiness and Health-State Policy

## Status

Accepted.

## Context

Vision-Core now validates important configuration boundaries and binds its P2P
and HTTP listeners before reporting successful service startup. It still lacks
a typed operational contract distinguishing process liveness, role readiness,
network condition, chain usability, mining condition, degradation, and fatal
startup failure.

A single Boolean would conflate locally valid chain state with external peer
availability and would make orchestration unsafe. Readiness data must also
remain outside consensus, protocol, wire, serialization, mining-algorithm, and
chain-database behavior.

## Decision

Vision-Core will design future operational state around these rules:

- component states use `starting`, `ready`, `degraded`, `not_ready`, `failed`,
  and `disabled`, with a separate stable reason code and human-readable
  explanation;
- liveness reports whether the process can respond and remains successful
  during degradation;
- readiness is evaluated against an explicit declared node role;
- non-role-critical degradation returns HTTP 200 from readiness, while a
  role-critical impairment returns HTTP 503;
- a normal networked node becomes degraded after an explicit zero-peer grace
  period and does not terminate solely because it remains peerless;
- isolated and inbound-only operation are distinct explicit modes; an isolated
  node can be ready with zero peers;
- at least one seed connection attempt begins before applicable readiness
  evaluation, but successful connection is not required;
- malformed seeds fail configuration loading, while unreachable valid seeds
  are runtime degradation evidence;
- public-node mode requires a complete syntactically valid advertised identity
  and an explicit override for private or loopback identities, but makes no
  claim of external reachability;
- mining configuration and initialization failures are fatal for roles that
  require mining; later worker failure uses bounded retry, degrades a node when
  mining is optional, and makes it not ready when mining is role-critical;
- new operational endpoints begin at `/api/v1/health`, `/api/v1/ready`, and
  `/api/v1/status`, carry an explicit schema version, and preserve existing
  field meanings within `v1` while allowing additive fields;
- transition history is bounded, in-memory, reset on restart, and initially
  defaults to 100 entries;
- structured logs are the durable integration surface;
- diagnostic values and unknown fields are sensitive by default, with explicit
  allowlisting and API redaction for paths, peer addresses, and rejected
  values.

The zero-peer grace duration, mining retry limit, and transition-history
capacity are named configuration/default decisions rather than hidden
constants. Public setting names, units, bounds, and migration behavior are
reviewed with the implementation tranche that introduces them.

## Consequences

The model can distinguish a live process from a node that can perform its
declared role. Temporary network loss does not trigger process termination,
and intentional isolation is not mistaken for failure. Mining can be required
for one role without making it a universal node-readiness prerequisite.

Monitoring clients receive stable state and reason codes rather than parsing
human logs. Versioned endpoints create a compatibility obligation: existing
`v1` meanings cannot be silently changed, and consumers must tolerate additive
fields.

Operational history consumes bounded memory and disappears on restart. Durable
diagnostics depend on the operator's structured-log retention. Redaction may
limit remote troubleshooting detail, but avoids exposing unknown or sensitive
values by default.

This decision authorizes design only. It does not authorize implementation,
new routes, new configuration settings, service-lifecycle changes, persistence
changes, or Desktop integration.

## References

- [Readiness and Health Model Design](../READINESS_HEALTH_MODEL_DESIGN.md)
- [Current-State Assessment](../VISION_CORE_CURRENT_STATE_ASSESSMENT.md)
- [Consensus Boundaries](../CONSENSUS_BOUNDARIES.md)
- [Testing Policy](../TESTING_POLICY.md)

# Vision-Core Readiness and Health Model Design

## Status and scope

This document is a design brief only. It does not authorize or implement a
status model, HTTP route, logging change, service-lifecycle change, persistence
change, or Desktop integration.

The current repository already exposes a useful `/status` snapshot and now
binds required listeners before reporting all services started. It does not yet
have one typed model connecting startup progress, local readiness, network
condition, chain usability, mining condition, degraded operation, and fatal
startup failure.

Any future model must describe operational state only. It must not affect
consensus, protocol versions, wire formats, persistence formats, chain validity,
or mining rules.

## State vocabulary

The model must be able to distinguish these observations without collapsing
them into one Boolean:

| Observation | Meaning |
| --- | --- |
| Process launched | The executable has entered its process/runtime startup path. |
| Configuration loaded | All required configuration sources parsed and validated successfully. |
| Chain database opened | The selected `chain.db` location opened successfully. |
| Chain state recovered | Genesis identity, snapshot/replay, indexes, cumulative work, and canonical state completed sufficiently for local use. |
| HTTP listener bound | The configured HTTP socket is reserved and ready to serve. |
| P2P listener bound | The configured P2P socket is reserved and the listener task can accept connections. |
| Seed dialing started | Outbound attempts for configured seeds have been scheduled or begun. |
| Peer count zero | No compatible peers are currently connected; this is not by itself a fatal local-startup failure. |
| Peers connected | At least one compatible peer session is active. |
| Synchronization idle | No synchronization operation is active; this may mean caught up, no target, or waiting to retry and therefore needs a reason. |
| Synchronization active | A specific peer/target synchronization operation is in progress. |
| Synchronization degraded | Progress is stalled, retrying, targetless, or limited while local state remains usable. |
| Mining disabled | Mining is not configured/enabled; general node readiness is unaffected. |
| Mining enabled | Mining is configured and a miner manager is available; active work remains a separate observation. |
| Fully operational | Critical local initialization succeeded and the enabled subsystems are operating within their approved policy. This must not silently imply a particular peer count or chain freshness policy. |

## Liveness

Liveness answers only: is the process alive and able to respond?

A future `GET /health` should be intentionally shallow. If the HTTP server can
serve the request and the event loop is responsive, it should normally return
success. It should not acquire long-lived chain locks, contact peers, wait for
storage operations, require synchronization, or require mining.

Liveness is not evidence that configuration loaded, storage recovered, the P2P
listener bound, peers connected, or the chain is current. If the HTTP listener
does not exist yet, an external caller cannot observe this endpoint; process
supervision remains the only pre-HTTP liveness signal.

## Startup readiness

Startup readiness means critical local initialization succeeded. At minimum it
requires:

- configuration is valid;
- the intended data directory was selected without fallback;
- storage opened successfully;
- chain state loaded or recovered successfully;
- required local listeners bound successfully.

Reachable seeds and connected peers should not be required by default. A node
can be intentionally isolated, temporarily offline, or waiting for peer
discovery while its local chain and services remain usable. Network condition
belongs in a separate dimension and may make the overall presentation
degraded.

The current startup order creates an API-observation question: HTTP is one of
the required listeners, yet `/ready` can only be queried after it binds. The
initial endpoint may therefore expose the already-completed readiness result,
while pre-bind progress remains observable through process supervision and
structured lifecycle logs.

## Network readiness

Network readiness should report facts rather than infer consensus safety from
reachability. Proposed fields include:

- P2P listener bound;
- configured bind address;
- whether advertised identity is configured;
- configured seed count;
- seed dialing started;
- successful seed connections;
- active inbound, outbound, durable, transient, and dialable peer counts;
- last peer or seed error, with timestamp and bounded non-secret context;
- last successful peer activity;
- best known remote height and cumulative work where available;
- synchronization target availability;
- current retry or backoff state.

Advertised peer work remains discovery information, never authority. Network
readiness must not bypass local validation or describe a peer-advertised chain
as accepted.

## Chain readiness

Chain readiness should describe whether local canonical state is usable and how
it relates to known network information. Proposed fields include:

- current canonical height;
- canonical tip hash;
- cumulative work at the canonical tip;
- cached state-root height and root where available;
- startup recovery state and last recovery result;
- synchronization state and reason;
- current target peer/height/work where available;
- last accepted block time if maintained reliably;
- a typed local-chain-usable flag derived from completed storage and recovery,
  not from peer claims.

“Usable” must not be presented as “globally current.” A node with no peers may
have valid local state without evidence that it has learned the highest-work
available chain.

## Mining readiness

Mining readiness is an independent dimension. Proposed fields include:

- configured;
- enabled;
- miner address present and valid under the approved future policy;
- worker configuration resolved;
- miner manager available;
- current work active or paused;
- pause or degradation reason;
- blocks found and other already supported statistics.

Mining disabled is a normal node mode. Mining failure should not make a
non-mining node generally unready. The treatment of failure after mining is
explicitly enabled remains an owner policy decision.

## Degraded operation

Degraded means critical local services remain usable but an operational
capability is impaired or lacks external confirmation. Candidate degraded
conditions include:

- zero peers after local startup;
- all configured seed dials failing;
- a stale or low-quality peer set;
- synchronization retrying, stalled, or lacking a viable target;
- higher-work recovery limited while local state remains readable;
- optional mining unavailable or paused;
- diagnostics that cannot determine freshness without invalidating local
  chain usability.

Degradation should contain typed reasons, timestamps, first/last occurrence,
and bounded diagnostic context. Multiple reasons should be representable
without relying on log-message parsing.

## Fatal startup failures

The following conditions prevent startup readiness and should terminate the
startup attempt with structured errors:

- invalid configuration;
- unusable or mismatched storage location;
- chain database open failure;
- genesis, snapshot, replay, or chain recovery failure;
- required P2P listener bind failure;
- required HTTP listener bind failure;
- unrecoverable construction failure for a service required by the selected
  node mode.

Fatal startup state should be logged and returned to the process boundary. It
does not need to remain queryable through an HTTP server that never bound.
External supervisors and Vision Desktop need stable exit/error classification
rather than only human prose.

## Proposed status model

Names must be reviewed against repository conventions before implementation.
A likely internal shape is a shared snapshot composed from distinct domains:

```text
NodeOperationalStatus
  lifecycle: NodeLifecycleState
  services: ServiceReadiness
  network: NetworkReadiness
  chain: ChainReadiness
  mining: MiningReadiness
  degraded_reasons: [DegradedReason]
  fatal_startup_reason: FatalStartupReason?
  observed_at: monotonic/wall-clock metadata as appropriate
```

Possible lifecycle states are `Launching`, `LoadingConfiguration`,
`OpeningStorage`, `RecoveringChain`, `BindingServices`, `Ready`, `Degraded`,
and `Stopping`. Fatal failure is better represented as a typed terminal reason
returned from startup than as a long-lived in-process state.

The model should use snapshots or atomics appropriate to each field, avoid
holding consensus/state locks across HTTP serialization, and keep operational
status updates outside canonical state. Status transitions must be deterministic
at their local boundaries even though peer availability is inherently dynamic.

## API implications

The clearest future contract is three distinct surfaces:

- `GET /health`: process liveness only;
- `GET /ready`: critical local startup readiness, with a small typed failure or
  degradation summary;
- `GET /status`: detailed operational state across chain, network, recovery,
  mining, and diagnostics.

Extending the existing `/status` response preserves its role but requires an
API compatibility decision because fields are manually documented and no
versioning policy exists. New `/health` and `/ready` endpoints avoid overloading
the existing snapshot, but route addition and response schemas still require
separate authorization and contract tests.

Recommended semantics:

| Endpoint | Success criterion | Must not require |
| --- | --- | --- |
| `/health` | HTTP process can respond | peers, sync, mining, chain freshness |
| `/ready` | configuration, storage/recovery, and required local listeners succeeded | reachable seeds, nonzero peers, mining |
| `/status` | always returns the best detailed snapshot once API serving is available | a healthy or ready conclusion |

The HTTP status-code policy, response versioning, authentication exposure, and
whether degraded readiness returns success or a non-2xx code are owner/API
decisions.

## Desktop implications

Vision Desktop should consume typed Core status rather than parse logs. It
could map Core state to user-facing states:

| Desktop state | Core evidence |
| --- | --- |
| Starting | Process launched but local startup readiness is incomplete. |
| Ready | Critical local initialization and required listeners succeeded. |
| Syncing | Ready locally and synchronization is active. |
| Degraded | Ready locally with one or more typed degraded reasons. |
| Offline | Process absent or liveness unavailable. |
| Mining | Ready with mining enabled and active. |
| Error | Fatal startup failure or a later unrecoverable service failure. |

Desktop should display peer count, sync target, local height/work, seed errors,
and mining status without presenting peer claims as accepted chain state. Core
remains the authority for consensus and persistence.

## Compatibility boundaries

The design and any future implementation must not change:

- block or transaction validity;
- proof-of-work or VisionX behavior;
- chain selection or state transitions;
- protocol versions or handshake compatibility;
- P2P wire messages or canonical serialization;
- database keys, stored shapes, snapshots, or state roots;
- mining rules or candidate validity.

Operational status is derived observation. It must never become an alternate
consensus input or persisted source of chain truth.

## Policy decisions required

The following require owner review before implementation:

1. Whether zero peers after startup is `Ready` with a degraded network state or
   changes the top-level result to `Degraded`.
2. Whether seed dialing begins before readiness is announced, provided dialing
   success remains non-blocking.
3. Whether API readiness always requires P2P listener success for every
   supported node mode.
4. Whether the HTTP listener itself is a readiness prerequisite or only the
   mechanism used to expose a previously completed local readiness state.
5. How intentionally isolated nodes declare that zero peers is expected.
6. Whether advertised identity is required for a node to claim public-network
   readiness while remaining optional for local or outbound-only modes.
7. Whether mining failures degrade only mining or the full node when mining was
   explicitly enabled.
8. Whether degraded `/ready` responses use success or non-success HTTP status.
9. Which status fields and error codes become versioned API contracts.
10. Retention, privacy, and redaction policy for last peer/seed errors.

## Proposed implementation tranches

No tranche below is authorized by this document.

### A. Internal lifecycle and status model

Define private typed state and transition tests. Do not add routes or change
service behavior. Characterize the current startup and recovery path first.

### B. Status propagation from storage and service startup

Update the internal model at configuration, storage, recovery, P2P bind, and
HTTP bind boundaries. Preserve current success and failure behavior exactly.

### C. Health and readiness HTTP endpoints

Add separately reviewed `/health` and `/ready` contracts, or an owner-approved
alternative. Keep detailed `/status` compatibility changes isolated.

### D. Network and seed diagnostics

Add bounded seed-attempt, peer-freshness, sync-target, and last-error status.
Do not make peer claims authoritative or silently change retry policy.

### E. Desktop integration

Publish a supported contract and update Vision Desktop to consume typed state,
exit classifications, and diagnostics without log parsing.

### F. Operational soak validation

Exercise clean startup, occupied listeners, invalid configuration, storage
failure, isolated mode, unreachable seeds, peer churn, sync recovery, restart,
and Desktop-supervised lifecycle across supported deployment environments.

Each tranche needs its own authorization, commit boundary, risk classification,
focused tests, full validation as required by [Testing Policy](TESTING_POLICY.md),
short-lived review branch, exact-head CI, promotion gate, and documentation
closeout.

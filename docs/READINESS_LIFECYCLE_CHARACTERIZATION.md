# Readiness Lifecycle Characterization

## Status and scope

This document characterizes the startup boundaries present at Vision-Core
commit `03550e64c3807585051672122841013d9ab66030`. It supports the first
readiness characterization tranche and does not authorize or implement runtime
status propagation, routes, settings, logging changes, service reordering, or
Desktop integration.

The accompanying private typed model is compiled only for tests. It is
executable characterization evidence, not a production status source.

## Current successful startup path

| Order | Observable boundary | Repository evidence | What the boundary proves |
| --- | --- | --- | --- |
| 1 | Process launched | `main`; `node::runtime::build_runtime` | The executable entered startup and constructed its Tokio runtime. |
| 2 | Configuration loaded | `Settings::from_env()?` in `async_main` | Required environment input parsed and current configuration validation passed. |
| 3 | Chain ready | `node::bootstrap::initialize_chain_state(&settings)?` returns | The data directory was prepared, sled opened, genesis/bootstrap completed, snapshot handling completed, canonical tail replay or rebuild completed, and the cached state root was refreshed. |
| 4 | P2P services started | `node::services::start_services(...).await?` returns | The P2P socket bound successfully and listener/background tasks were spawned. |
| 5 | HTTP listener bound | `TcpListener::bind(http_addr).await?` returns | The configured API socket was reserved successfully. |
| 6 | Operational startup reported | `[NODE] All services started` is emitted | Both required listener binds and the preceding local initialization boundaries succeeded. |
| 7 | HTTP serving | `axum::serve(http_listener, app).await` | The API server owns the already-bound listener until shutdown or failure. |

## Boundaries that are not separately observable

The present call graph does not provide independent caller-visible completion
events for every approved future status field:

- database-open success and chain-recovery success are combined inside
  `initialize_chain_state`;
- snapshot restore, canonical replay, and full rebuild are internal recovery
  paths rather than distinct operational-state transitions;
- `start_services` proves that seed tasks were spawned, but task scheduling
  does not prove that at least one outbound connection attempt began before the
  function returned;
- mining task creation does not prove that mining is active, because mining can
  remain paused for peer count, recovery state, or unavailable work;
- no explicit node role, isolated mode, inbound-only mode, or public-node mode
  exists in the current settings model;
- no shared runtime object records lifecycle or component transitions;
- fatal startup failures propagate through `Result` and terminate at the
  process boundary; they are not retained as queryable in-process state.

Future propagation must add observations at the owning boundary. It must not
derive a stronger claim from task creation, log order, peer advertisement, or
the absence of an error.

## Characterized fatal startup boundaries

| Boundary | Current failure behavior |
| --- | --- |
| Runtime construction | Prints a fatal error and exits before `async_main`. |
| Configuration loading | Returns an error from `Settings::from_env`; process exits. |
| Data-directory preparation or database open | Returns a structured storage error; process exits before services. |
| Genesis, snapshot, replay, or rebuild | Returns an error; process exits before services. |
| P2P listener bind | `start_services` returns an error; API bind and successful-startup reporting do not occur. |
| HTTP listener bind | `async_main` returns an error; successful-startup reporting does not occur. |

The test-only model represents these boundaries with typed failure categories.
It does not change current error messages, exit codes, or control flow.

## Test-only typed model

`src/node/readiness.rs` characterizes:

- the approved component-state vocabulary: `starting`, `ready`, `degraded`,
  `not_ready`, `failed`, and `disabled`;
- the currently observable successful startup order;
- rejection of out-of-order transitions without state mutation;
- the component affected by each characterized fatal boundary;
- the distinction between no configured seeds, spawned seed tasks, and a
  connection attempt that has actually begun;
- the fact that spawning mining work does not prove mining readiness.

The module is included through `#[cfg(test)]`. Production code does not
construct, update, lock, serialize, log, or expose this model.

## Deferred implementation boundaries

The following remain outside this tranche:

- production lifecycle/status storage;
- status propagation from configuration, storage, recovery, listeners, seeds,
  synchronization, or mining;
- role and mode configuration;
- zero-peer grace timing;
- mining retry limits;
- transition-history allocation;
- stable reason-code definitions;
- `/api/v1/health`, `/api/v1/ready`, or `/api/v1/status`;
- changes to the existing unversioned `/status` route;
- structured lifecycle logging;
- Vision Desktop consumption.

The next tranche should promote an owner-reviewed subset of the test model into
private production state and update it at existing boundaries without changing
their success, failure, ordering, or timing behavior.

## Compatibility classification

This characterization changes no runtime behavior. It has no consensus,
protocol, wire-format, serialization, persistence, snapshot, state-root,
mining-algorithm, API, configuration, or database-format impact.

See [Readiness and Health Model Design](READINESS_HEALTH_MODEL_DESIGN.md) and
[ADR-0009](DECISIONS/0009_readiness_health_state_policy.md) for the approved
policy that governs future work.

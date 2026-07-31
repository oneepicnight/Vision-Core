# Vision-Core Current-State Assessment

Assessment date: Friday, July 31, 2026

## Executive assessment

Vision-Core is a validated blockchain engine with a disciplined repository and
release process. It is now an active operational-hardening project rather than
a repository-modernization project. It is not yet a complete user-facing
product and this assessment does not classify it as generally production-ready.

The strongest evidence is concentrated in consensus preservation, proof-of-work
compatibility, chain acceptance, restart and reorganization behavior,
deterministic synchronization testing, and exact-commit promotion. Recent work
has also made invalid configuration and required-listener failures more honest
at startup. The largest operational gap is the absence of a formal lifecycle,
readiness, degraded-state, and fatal-state model that can be consumed
consistently by operators, automation, and Vision Desktop.

## Repository state

| Item | Verified state |
| --- | --- |
| Promoted `origin/main` | `da72260dcb9cb66971c433e05a6633a4800dfe4d` |
| Promoted tree | `457efc2647bf56fbfc29381cf3796410b7533775` |
| Long-lived integration branch | `dev/configuration-hardening-v104`, synchronized locally and remotely to the promoted main commit before this documentation work |
| Historical release tag | `vision-core-consensus-v1.0.4` |
| Historical release commit | `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Historical release tree | `d650bb3419db56cce9e1d789611763e5cb4cbc26` |

The historical release tag has not moved. Current `main` contains later
developer-readiness and operational-hardening work, so the release tag and the
current promoted development baseline must not be treated as the same identity.

The repository uses small commits, short-lived review branches, pull requests,
exact-head CI, explicit promotion gates, and ordinary fast-forward history.
The promoted startup-sequencing baseline recorded a clean worktree, 544 passing
release tests, no failures, one ignored test, one passing focused watchdog
test, and 43 passing VisionX tests. The ignored test remains
`node::bootstrap::tests::bootstrap_recovery_worker`.

## Maturity matrix

Ratings describe repository evidence at the promoted main baseline. They are
not product certification.

| Area | Rating | Evidence and limitation |
| --- | --- | --- |
| Consensus safety | Strong | Unified block acceptance, protected consensus boundaries, immutable historical identity, and expanded validation are documented in [Consensus Boundaries](CONSENSUS_BOUNDARIES.md), [Architecture Overview](ARCHITECTURE_OVERVIEW.md), and accepted decision records. No later hardening tranche changed consensus. |
| Proof of Work | Strong | Historical VPoW encoding and target behavior remain protected by exact compatibility decisions and focused proof tests. No current task authorizes redesign. |
| VisionX | Strong | VisionX has deterministic cache and compatibility rules plus a focused suite; 43 tests passed at the current promoted validation baseline. |
| Chain acceptance | Strong | Peer, synchronization, orphan, and mined blocks share `chain::accept::apply_block`; rejection and atomicity cases are covered in source tests. |
| Reorganization handling | Strong | Cumulative-work fork choice, branch replay, state restoration, and focused reorganization coverage are present. Tranche 4B recorded 16 passing reorganization tests. |
| Persistence | Strong | Startup recovery, snapshots, state roots, and restart behavior have dedicated boundaries and suites. Database migration and downgrade policy remain unresolved, so this rating does not imply format-evolution readiness. |
| P2P synchronization | Improving | Deterministic watchdog recovery, higher-work synchronization, handshake checks, and multi-node coverage are strong. Network status, seed failure history, and operational freshness are not yet modeled comprehensively. |
| Configuration validation | Improving | Runtime threads, data-directory selection, private-peer policy, seed syntax, and advertised identity now fail early where invalid. HTTP/P2P ports, mining settings, alpha enablement, and `VISION_CONFIG` still contain unresolved or fallback behavior. |
| Startup correctness | Improving | P2P and HTTP bind failures now abort startup before `[NODE] All services started`. Storage and recovery failures already propagate. A typed lifecycle and readiness contract is still absent. |
| Health/readiness observability | Incomplete | `/status` exposes useful chain, peer, mining, and recovery snapshots, but there is no explicit liveness endpoint, readiness endpoint, lifecycle state, degraded reason, or fatal-startup status model. |
| Seed diagnostics | Incomplete | Seed syntax is validated before startup, but reachability failures remain asynchronous and there is no durable configured/successful seed count, last seed error, or retry-status contract. |
| Mining configuration | Incomplete | Mining can be enabled, but invalid miner identity still falls back and `VISION_MINING_THREADS` is parsed without a runtime consumer. Required mining-policy decisions remain open. |
| HTTP API contract | Improving | Routes and deterministic response examples are documented and tested in places. Error envelopes and not-found semantics differ, `/peers` is a stub, and no OpenAPI or explicit versioning contract exists. |
| Structured logging | Improving | The repository uses `tracing` and startup diagnostics are clearer. Stable structured fields, subsystem conventions, and machine-consumable lifecycle events are incomplete. |
| Developer documentation | Strong | `AGENTS.md`, controlling policies, architecture, configuration, testing, release, roadmap, decisions, and playbooks form a navigable engineering operating system. Living documents still require synchronization with every promotion. |
| CI and release governance | Strong | Blocking check, formatting, and single-threaded release jobs plus visible non-blocking Clippy reporting run on PRs and `main`; promotion and tag authority are explicit. |
| Warning/lint health | Improving | Compiler debt is classified at 58 normal-target and 30 test-target warnings. Clippy remains deliberately non-blocking. The ledger prevents compiler-driven removal across protected boundaries. |
| Desktop integration | Incomplete | Vision Desktop is a separate early node-manager foundation. A stable status/readiness contract and end-to-end private-Core integration evidence are not yet established here. |
| NAT traversal | Not yet addressed | No repository evidence establishes UPnP, NAT-PMP, PCP, STUN, TURN, relay, or automatic port-mapping support. Advertised identity is configuration-driven. |
| External closed-alpha readiness | Incomplete | Core validation is substantial, but supported deployment profiles, non-Windows validation, health/readiness automation, backup/restore runbooks, security intake, and distributed soak evidence remain incomplete. |
| Public-user readiness | Not yet addressed | Wallet, safe key lifecycle, stable API contracts, installers, platform support, monitoring, incident response, and product-level integration are not complete in Vision-Core. |

## Promoted work since v1.0.4

The immutable v1.0.4 tag remains at `b874d73c`. Later work was promoted to
`main` as new commits and did not retag that release. Promoted work includes:

- Developer Foundation documentation, governance, toolchain, release identity,
  and baseline CI;
- an isolated formatting and warning baseline;
- dead-code classification followed by only owner-approved test-infrastructure
  and audited ChainState cleanup;
- configuration characterization and a typed settings translation seam;
- strict positive-integer validation for `TOKIO_WORKER_THREADS`;
- characterization and fail-fast validation of `VISION_DATA_DIR` before sled
  opens, without changing `chain.db` layout or persisted formats;
- P2P configuration loading that preserves the established private-peer
  default, validates explicit booleans, distinguishes omitted and explicitly
  empty seeds, rejects malformed seed entries, and requires complete advertised
  identity;
- startup sequencing that binds P2P before detaching its listener task, treats
  required P2P/API bind failures as startup failures, and delays successful
  startup reporting until both listeners bind.

These changes improve operator correctness and repository maintainability.
They do not constitute a new tagged consensus release and do not claim changes
to consensus, protocol versions, wire formats, persistence formats, chain
validity, or mining rules.

## Current strengths

- Consensus-sensitive work has explicit classification, authorization, and
  expanded validation boundaries.
- Review branches, pull requests, exact-commit CI, and fast-forward promotion
  create a clear and auditable change history.
- Hardened configuration paths fail early with actionable errors instead of
  silently selecting unintended runtime state.
- Required listener startup is truthful: a bind failure cannot coexist with a
  successful all-services-started message.
- Operator and contributor documentation now explains implemented behavior,
  unresolved policy, and historical identity separately.
- The current promoted code baseline is clean under the recorded cargo check,
  formatting, focused, VisionX, watchdog, multi-node, and full-release gates.

## Current risks and gaps

- Readiness, health, lifecycle, degradation, and fatal-state policy is now
  formally defined, but no typed runtime model or endpoint implements it.
- Seed connection diagnostics remain separate from listener readiness and are
  not exposed through a stable operational status contract.
- Mining configuration policy remains unresolved, including miner identity and
  the ownership of `VISION_MINING_THREADS`.
- The HTTP contract is manually documented rather than machine checked; error
  envelopes and status semantics remain inconsistent.
- Structured logging is incomplete for stable lifecycle, peer, synchronization,
  storage, and mining fields.
- Warning and dead-code debt remain classified but intentionally not eliminated.
- External deployment, distributed soak, incident recovery, and supported
  non-Windows validation remain incomplete.
- Vision Desktop remains separate and early, without a finalized readiness and
  diagnostics contract from Core.

## Recommended next priorities

These priorities are recommendations, not implementation authorization.

1. **Internal readiness model characterization.** Starting from the approved
   readiness and health-state policy, characterize current lifecycle boundaries
   and propose the first private typed model without adding endpoints or
   changing runtime behavior.
2. **Seed-connection diagnostics and status reporting.** Record configured
   seeds, dialing state, successful connections, last errors, peer freshness,
   and sync-target availability without making external reachability a local
   startup prerequisite.
3. **Mining configuration policy.** Decide when a miner address is required,
   how invalid input fails, and whether `VISION_MINING_THREADS` is implemented,
   renamed, or removed.
4. **HTTP error and API contract standardization.** Define versioning, stable
   error envelopes, status semantics, `/peers`, and a machine-readable contract.
5. **Desktop/private-Core integration and distributed rehearsal.** Exercise
   lifecycle, readiness, restart, recovery, peer loss, and upgrade behavior
   through the intended local application boundary and multiple independent
   node environments.

## Overall verdict

Vision-Core is a well-validated blockchain engine with strong consensus and
repository-governance foundations. It has progressed materially from merely
passing tests to failing more honestly at configuration and service-startup
boundaries. Its next maturity constraint is operational truth: the software
needs a typed, observable distinction between alive, locally ready,
network-connected, synchronized, degraded, mining, and fatally failed states.

The readiness-design policy is approved. The project is ready for separately
authorized, narrow characterization and implementation tranches; the design
itself authorizes no runtime change. Vision-Core is not yet a complete operator
platform, closed-alpha package, or public-user product.

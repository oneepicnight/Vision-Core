# Three-Node Mining, Relay, and Recovery Validation Report

## Executive summary

Vision-Core passed the authorized local three-node distributed-network
validation on 2026-08-01. Three independently persisted nodes formed a direct
mesh, mined competitively for more than one hour, relayed blocks from every
miner, retained B-to-C operation while seed node A was offline, recovered A
from its persisted state, resolved a controlled fork, reached stable agreement,
and restored that agreement after a full process restart.

The run reproduced none of the original persistent session-collapse behavior.
During 360 ten-second mining samples, the three-node mesh had zero full-mesh
zero-peer samples and produced 306 exact convergence samples. All nodes ended
the one-hour relay interval at height 150 with the same tip and state root.

Final classification: **A. Session ownership and mining recovery validated in
full three-node rehearsal.**

This is local multi-process evidence. It is a strong prerequisite for the
planned four-computer Internet soak, but it is not a substitute for NAT,
router, WAN-latency, public-address, or 48-hour endurance evidence.

## Scope and immutable baseline

| Item | Value |
| --- | --- |
| Repository | `C:\vision\Vision-Core` |
| Branch | `dev/p2p-session-stability-v104` |
| Commit | `8686bbd44689538e53020e4a3d547d57f73949be` |
| Tree | `e98e831239d8d350448566c79e0d07a8fbfad25a` |
| Release identity | `v1.0.4` |
| Historical v1.0.4 tag target | `b874d73cbdf60657334b62c867ed7f18b80a186b` |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Cargo | `cargo 1.97.1 (c980f4866 2026-06-30)` |
| Binary SHA-256 | `E8AAB64179DDF6CDFA100058C6AE8BF78C44F99FD75C298346EB8C589F1FBC33` |

No Vision-Core source, Cargo, CI, dependency, consensus, protocol, database,
or tag state was changed by this validation.

## Topology and isolation

The primary run used three processes and three independent data directories:

| Node | P2P | HTTP | Initial seed | Miner identity |
| --- | ---: | ---: | --- | --- |
| A | 64330 | 64430 | Explicit empty seed list | `11…11` (64 hex characters) |
| B | 64331 | 64431 | `127.0.0.1:64330` | `22…22` (64 hex characters) |
| C | 64332 | 64432 | `127.0.0.1:64330` | `33…33` (64 hex characters) |

B and C were not configured with each other as seeds. Peer exchange produced a
direct B-C session, and that direct session remained after A was stopped.

## Results by phase

### 1. Idle discovery mesh

- Result: passed.
- Samples: 60 at ten-second intervals.
- Observed duration: 591.67 seconds between the first and last sample.
- Peer count: two on every node throughout the stable interval.
- Full-mesh zero-peer samples: 0.
- Session-collapse events: 0, using the original simultaneous zero-peer
  signature as the classification boundary.
- Session replacements: no collapse-inducing replacement was observed. The
  current status surface does not expose a distinct replacement counter;
  connection-establishment log lines also include short-lived synchronization
  connections and are therefore not reported as session replacements.
- Stable direction topology before mining: A accepted two inbound sessions, B
  held one inbound and one outbound session, and C held two outbound sessions.

This proves seed-assisted discovery produced a complete three-node topology,
including the required B-C direct relationship.

### 2. Three-miner relay

- Result: passed.
- Samples: 360 at ten-second intervals.
- Observed duration: 3,620.53 seconds (60 minutes, 20.53 seconds).
- Exact convergence samples: 306.
- Full-mesh zero-peer samples: 0.
- Final peer count: A=2, B=2, C=2.
- Final blocks-found counters: A=72, B=64, C=56.
- Final height: 150 on all nodes.
- Final tip:
  `00afc69396a5b7c7a1bc36675fb393991f13664085f787f37802dddfad854c5e`.
- Final state root:
  `e4678876c43a1f791a392164b013abdfcaed348a16951623b287c4b8e8fce896`.

Every node logged locally accepted mined blocks, and every node logged blocks
accepted from peers. Across the complete evidence set, the node logs contain
85, 78, and 70 locally accepted mined-block records for A, B, and C,
respectively. The same logs contain 127, 115, and 162 peer-attributed block
acceptance records. Some locally found blocks correctly landed on side chains
during simultaneous competition; cumulative-work selection repeatedly restored
a common canonical chain.

### 3. Seed independence

- Result: passed.
- A was stopped at height 150.
- B and C retained one direct peer each.
- B advanced its blocks-found counter from 64 to 68.
- C advanced its blocks-found counter from 56 to 59.
- B and C converged at height 157.
- Converged tip:
  `014f57a253dd665d4673f6ff3882fc73ab3ea120ed76531323f2e089ff571c3c`.
- Converged state root:
  `4842a0ad789b70fe8e3ec534db92bcf49ea5167b58a41156800fe0126fb71991`.
- Observed duration: 181 seconds.

The configured seed was therefore not a permanent hub or relay dependency.

### 4. A restart and mining recovery

- Result: passed.
- A restarted from its original data directory at height 150.
- A rejoined the two-peer mesh, caught up, returned to normal recovery state,
  resumed mining, and found a block.
- All three nodes converged at height 158 after 11.08 seconds of sampled
  recovery.
- Converged tip:
  `01faae7f667129e90b81f1429a27a569487d899876abd2c159cddcaab4397531`.
- Converged state root:
  `385f31d52a8a21db7e8ffdea1d9fbdfe8864a6b35a52185c19217f4386054f6b`.

### 5. Controlled partition and rejoin

- Result: passed.
- Method: transparent local TCP relay with block-propagation frames withheld
  while handshake, height, and keepalive frames remained available.
- Logical sessions remained established and all three miners remained active.
- A/B and C produced distinct canonical tips during the partition.
- Final retained partition duration: 142 seconds.
- Final recovery duration: 82 seconds.
- A/B partition tip:
  `009d9238f4d1e6dd66c22b9176e8303c7cab732e14a9fed8294dc4b02c93202f`.
- C partition tip:
  `025c8e2df76e28fcc3b0e218f233be4d9c489118078b2e20cf1f886bb75dd228`.
- Rejoined height: 177.
- Rejoined tip:
  `00042d55ad92054658b30732362d964060202058ca7342100005f64b39cf53ac`.
- Rejoined state root:
  `f099273618a1df44e9fabadceae6e457585ec26d18e5c47bf7188a2941692ac5`.

The final retained partition recovery recorded a maximum reorganization depth
of 2. An earlier valid partition/recovery observation in the preserved harness
evidence recorded depth 3. The maximum reorganization depth across the entire
competitive mining run was 20 during the initial multi-miner startup race from
genesis. All recorded reorganizations completed successfully.

### 6. Stable final convergence

- Result: passed.
- Stable consecutive samples: 10.
- Height: 177.
- Tip:
  `00042d55ad92054658b30732362d964060202058ca7342100005f64b39cf53ac`.
- Cumulative work: 21,401 on all nodes.
- State root:
  `f099273618a1df44e9fabadceae6e457585ec26d18e5c47bf7188a2941692ac5`.
- Version: `v1.0.4` on all nodes.
- Peer counts: A=2, B=2, C=2.

### 7. Full persistence restart

- Result: passed.
- All node listener and API ports closed before restart.
- All nodes restarted from their existing data directories at or above the
  height-177 persisted checkpoint.
- Full-mesh discovery returned.
- All three mining workers resumed and each found at least one new block.
- Nodes reconverged after 52 seconds.
- Final height: 181.
- Final tip:
  `019bf7b7ffddd489406bda57d407fd67ff9770139b6eff5938ce7aff9c9735a6`.
- Final cumulative work: 21,694. B and C logged this value directly; A's
  identical final height and tip imply the same deterministic cumulative work.
- Final state root:
  `f5ba7a3f3c8dec1715bedd97bbb4f63e796d2dfa3d85867cb2b5030f36c7e352`.

## Original failure comparison

The original run at
`C:\vision\test-runs\three-node-mining-relay-20260801-085933` used baseline
`25a7619d324e763a43dbd53f9011de48ca80aead`. It ended with B and C at zero
peers, A at one peer, and non-converged state: A/B were at height 28 while C
was at height 27 with a different tip and state root. Its samples included five
simultaneous full-mesh zero-peer observations.

The present run used the promoted session-ownership and mining-recovery fixes.
It produced no full-mesh zero-peer sample during either the idle or one-hour
mining intervals, maintained direct B-C operation after A stopped, restored A,
recovered from an intentional fork, and passed full persistence restart. The
original repeated collapse signature was not reproduced.

## Harness notes

All harness changes were confined to the evidence directory and did not touch
the repository. Failed harness attempts were preserved rather than erased:

1. The initial runner encountered duplicate Windows `Path`/`PATH` entries
   before any node started.
2. Two proxy-readiness probes were invalid because `Get-NetTCPConnection`
   returned access denied for the Python relay; `netstat` supplied the corrected
   non-mutating probe.
3. A stale evidence PID filename was consumed by a repeated attempt; unique
   timestamped PID names corrected it.
4. A byte-total blackhole demonstrated the five-second P2P keepalive timeout and
   was replaced by a frame-aware block-propagation partition.
5. One successful partition/recovery reached ten stable convergence samples,
   then an evidence-only PowerShell cast typo stopped cumulative-work reporting.
   Cleanup passed, the attempt was preserved, and the final retained attempt
   repeated and completed every remaining gate.

These are classified as harness/infrastructure corrections. None changed node
code, configuration semantics, consensus parameters, proof of work, or stored
chain data. The final retained attempt has a null failure field and clean
cleanup evidence.

## Errors and resource observations

- Fatal errors: 0.
- Panics: 0.
- Unexpected Vision-Core process exits: 0.
- Full-mesh zero-peer samples in the stable idle and mining phases: 0.
- Log matches for socket-close/reset/early-EOF conditions across all retained
  attempts: 108. These include deliberate process stops, proxy experiments,
  reconnections, and normal shutdown boundaries; none caused the validated
  primary mesh to collapse.
- Keepalive timeouts across all retained attempts: 4, confined to the discarded
  coarse-blackhole harness attempt.
- Peak sampled working set across all retained attempts: A=1,211,293,696 bytes,
  B=1,291,112,448 bytes, C=1,270,317,056 bytes.
- Evidence footprint at report generation: 354,445,452 bytes across 1,339 files;
  log files accounted for 8,510,861 bytes.

The memory footprint is substantial for running three proof-of-work nodes on a
single host and should be treated as a capacity-planning input for the planned
four-computer soak. It did not cause a process exit in this run.

## Evidence

Primary evidence root:

`C:\vision\test-runs\three-node-mining-relay-recovery-20260801-120631`

Key files:

- `status-samples.ndjson` — phases 1 through 5 sampling.
- `harness-events.ndjson` — primary node and phase events.
- `continuation-status-samples.ndjson` — retained partition, convergence, and
  restart sampling.
- `continuation-events.ndjson` — retained continuation lifecycle.
- `continuation-summary.json` — successful partition/convergence/restart
  summary with a null failure field.
- `continuation-cleanup.json` — zero remaining nodes and zero occupied ports.
- `proxy-events.ndjson` — relay connection and withheld-traffic evidence.
- `A`, `B`, and `C` — configurations, PIDs, logs, and persisted chain databases.
- `continuation-attempt-*` — preserved harness-failure evidence.
- `binary-sha256.json` — exact tested executable identity.

## Repository state and recommendation

The tested commit and remote references remained unchanged throughout the run.
The only repository change produced by this task is this Markdown report.

Vision-Core is ready to proceed to the four-computer, 48-hour Internet soak,
subject to the existing WAN runbook and operator setup. That soak should retain
the same acceptance gates: direct non-seed peer discovery, block contribution
from every miner, no repeated mesh collapse, stable cumulative-work convergence,
restart recovery, bounded resource growth, and complete NAT/router evidence.

No source fix is indicated by this local validation.

# P2P Internet Soak Characterization

## Status and scope

This document records the engineering boundary for the planned 48-hour,
four-node rehearsal: two physical laptops, two virtual machines, all four
mining independently, and one laptop used as the initial seed.

The original characterization identified block dissemination as the blocking
runtime gap. The current review candidate addresses that gap without changing
block validity, cumulative-work fork choice, proof of work, VisionX,
serialization, genesis, persistence format, state-root calculation, or the P2P
protocol version and existing wire-message layout.

The candidate must still complete remote review, CI, promotion, and a short
four-node dry run before the 48-hour clock begins. Operational instructions are
in [P2P_48_HOUR_SOAK_RUNBOOK.md](P2P_48_HOUR_SOAK_RUNBOOK.md).

## Implemented dissemination path

The candidate uses the existing `AnnounceBlock`, `GetBlock`, and `Block` wire
messages to provide a bounded relay path:

1. a locally accepted canonical block is announced to live peer sessions;
2. a peer requests an unknown announced block;
3. the full block enters one bounded inbound queue;
4. the block passes through the ordinary local block-acceptance pipeline;
5. an accepted canonical block is announced onward, excluding its immediate
   source;
6. duplicate or already-known blocks do not create an unbounded relay loop.

Mining jobs carry the canonical parent identity. A peer block that advances the
canonical tip cancels stale local work so a miner does not continue competing
on an obsolete parent until the next polling interval.

## Discovery and connection lifecycle

Successful handshakes exchange a bounded list of dialable known peers. Newly
learned addresses enter a dialing supervisor that maintains multiple outbound
sessions. This is handshake-time exchange, not continuous peer-list gossip.
A connected star can still disseminate blocks even if leaves never become
directly connected to one another.

Each process generates a distinct node nonce, preventing separate machines
using the same listen address from being misclassified as self-connections.
Inbound disconnects clear peer height, tip, work, and connected state
immediately. Cleanup is generation-aware, so a delayed close from an older
session cannot disconnect a newer replacement session for the same peer.

## Automatic port behavior

`VISION_P2P_PORT=auto` derives a stable local TCP port in the range 20000
through 59999 from the routed local IP. `VISION_P2P_ADVERTISED_PORT=auto` uses
that resolved listen port when paired with an explicit advertised host. The
startup banner reports the selected P2P address.

This is local port selection only. Vision-Core does not discover the public IP,
open Windows Firewall, or establish UPnP, NAT-PMP, PCP, STUN, TURN, or relay
services. Router reachability remains an explicit operator responsibility.

## Topology implications

Two laptops on one household router share a public IPv4 address. The second
laptop should dial the seed over its LAN address unless NAT loopback has been
verified. Genuinely remote VMs can dial the public IPv4 address and the seed's
forwarded TCP port. VMs hosted behind the same router are not independent
internet paths.

Only the seed needs inbound public reachability for the planned star-assisted
rehearsal. A fully inbound-reachable mesh would need a distinct external port
mapping for each participant. Nodes that cannot accept inbound traffic should
not advertise an unreachable identity.

The HTTP API should remain local or explicitly protected. It has no built-in
TLS or authentication and is not required for P2P reachability.

## Preserved safety boundaries

- A peer announcement is never authority; received blocks undergo ordinary
  validation.
- Cumulative work remains the chain-selection rule.
- The deterministic synchronization watchdog remains active.
- No database, snapshot, or state-root format changes are included.
- No consensus, proof-of-work, VisionX, or mining-algorithm changes are
  included.
- Existing protocol identity and wire encoding remain unchanged.
- Per-session channels and shared-peer lists are bounded.

## Validation evidence

The local candidate passed:

- focused connection, peer-manager, service, synchronization, and mining tests;
- deterministic watchdog recovery;
- VisionX validation;
- `cargo check --all-targets --locked --offline`;
- formatting and diff checks;
- the single-threaded release suite after each isolated runtime concern.

At the current candidate tip, the release suite completed with 565 tests:
564 passed, 0 failed, and 1 ignored. The compiler-warning baseline is 57 normal
target warnings and 29 test-target warnings. The reduction from 58/30 is a
direct consequence of activating the previously unused outbound-peer target;
no lint suppression was added.

## Remaining operational risks

Remote CI and real multi-machine behavior cannot be proven by local unit and
integration tests. The dry run must specifically verify:

- router forwarding and Windows Firewall behavior;
- seed access from both the LAN and actual remote path;
- block propagation from every miner, including leaf-to-seed relay;
- convergence after simultaneous block discovery;
- reconnect and catch-up after peer and seed interruptions;
- restart from each existing `chain.db`;
- bounded CPU, memory, disk, and log growth.

Automatic port derivation is deterministic but not collision-proof. A routed
IP change can change the selected port. Peer lists are exchanged only on
handshake, and the current `/peers` endpoint is not connected to live state;
operators must use `/status` peer counters and structured logs for evidence.

## Entry verdict

The candidate closes the source-level dissemination blocker identified by the
original characterization. Do not call the system internet-soak validated yet.
First promote the exact reviewed candidate, obtain green post-promotion CI,
execute the 30-minute four-node dry run in the runbook, and confirm that blocks
mined by each participant reach and converge on all others. A successful dry
run is the final gate for starting the 48-hour rehearsal.

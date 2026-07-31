# P2P Internet Soak Characterization

## Status and scope

This document characterizes the promoted Vision-Core P2P implementation for a
planned 48-hour, four-node rehearsal. The intended participants are two
physical laptops and two virtual machines, with every node configured to mine
and one node serving as the configured seed.

This tranche does not change production behavior, consensus, protocol
versions, wire messages, synchronization policy, mining policy, persistence,
configuration, or startup sequencing. The added Rust assertion is test-only
and records existing inbound-message behavior.

## Readiness conclusion

The current implementation is suitable for controlled local synchronization
and reconnect testing, but it is **not yet ready for the proposed 48-hour
all-miner star-topology soak**.

The blocking issue is block dissemination. The runtime defines
`AnnounceBlock` and `Block` messages, but a successfully mined block is not
broadcast to connected peers. The inbound connection loop also does not import
an unsolicited announcement or block. Configured seed loops pull a
higher-work chain from the seed they dial, but the seed does not continuously
poll every inbound leaf for new work. A leaf miner can therefore advance its
own chain without reliably causing the seed or other leaves to learn about the
new block.

Starting the 48-hour all-miner run before this boundary is addressed could
produce persistent competing local chains while each process appears alive.

## Observed topology behavior

### Listener and startup

- P2P listens on `0.0.0.0` and the configured `VISION_P2P_PORT`.
- Failure to bind the P2P listener aborts startup before the node reports all
  services started.
- HTTP and P2P ports must be unique for nodes sharing one operating-system
  network namespace.

### Seed dialing

- `VISION_SEED_PEERS` accepts IP-literal socket addresses.
- An omitted value uses the compiled default seeds.
- An exactly empty value disables configured seeds.
- A configured seed task retries indefinitely with a two-second delay after a
  failed connection or handshake.
- Seed reachability is asynchronous and does not currently determine startup
  success.

### Handshake and identity

- Peers verify protocol, chain, genesis, economics, proof parameters, and
  self-connection identity during the handshake.
- Advertised identity is optional, but host and port must be supplied together
  when configured.
- Private, loopback, and link-local advertised identities are accepted under
  the preserved default `VISION_ALLOW_PRIVATE_PEERS=true` policy.
- The repository provides no automatic NAT discovery or port mapping through
  UPnP, NAT-PMP, PCP, STUN, TURN, or relay services.

### Synchronization and recovery

- Configured outbound seed sessions poll the seed's chain summary and pull
  blocks when the seed advertises greater cumulative work.
- Downloaded blocks still pass through ordinary local block validation.
- The deterministic watchdog can recover synchronization from a failed or
  malicious candidate peer when a valid higher-work peer is available.
- The existing multi-node suite demonstrates local convergence and reconnect
  through explicit synchronization operations.
- Current inbound sessions do not provide continuous reverse summary polling,
  and current mining success does not emit a block announcement.

### Mining

- Mining requires at least one connected peer.
- With a one-seed star, each leaf can satisfy that gate through its outbound
  seed session, and the seed can satisfy it through an inbound session.
- `VISION_MINING_THREADS` is parsed but is not currently consumed by the
  mining runtime.
- An invalid `VISION_MINER_ADDRESS` currently falls back to the zero address;
  mining-identity hardening remains a separate policy and implementation
  concern.
- Peer-count eligibility does not prove that locally mined work will propagate
  through the star topology.

## Network-addressing constraints

If all four participants are behind the same household router, they share one
public IPv4 address. That arrangement exercises the public address only when
the router supports NAT loopback, also called hairpin NAT. Without that
feature, local participants may be unable to reach the seed through the
router's public address even though an outside host could.

One external TCP port can forward to only one internal P2P listener. Giving all
four nodes inbound reachability therefore requires four distinct external TCP
ports mapped to the corresponding internal node ports. A seed-only forwarding
plan needs only the seed's external TCP port, but it leaves the topology with
the reverse-discovery and dissemination limitations described above.

A genuinely internet-routed rehearsal requires at least one participant on a
different upstream network. Locally hosted virtual machines using bridged or
NAT networking behind the same router do not create an independent internet
path. Cloud-hosted virtual machines would.

The HTTP API should not be exposed through the router for this rehearsal. It
has no built-in authentication or TLS. Vision Desktop should reach Core over a
local or otherwise explicitly protected interface.

## Evidence added by this tranche

`p2p::connection::tests::inbound_announcements_and_blocks_do_not_change_chain_without_sync`
performs a valid handshake, sends an announcement and a full block, uses a
Ping/Pong exchange as a deterministic processing barrier, and confirms that
the inbound path has not changed local chain state. The test records the
current boundary; it does not endorse that boundary as the desired design.

## Required implementation before the all-miner soak

The next behavior tranche should define and implement one coherent block
dissemination path:

1. announce a locally accepted canonical block to applicable connected peers;
2. handle announcements without trusting peer claims;
3. request or receive the announced block through a bounded path;
4. route the block through unified block acceptance;
5. avoid announcement loops and duplicate downloads;
6. preserve cumulative-work fork choice and deterministic watchdog recovery;
7. expose enough diagnostics to distinguish connected, synchronized, and
   partitioned miners.

That tranche is networking- and protocol-sensitive even if it reuses existing
wire variants. It requires separate owner authorization and the P2P validation
matrix in `TESTING_POLICY.md`.

## Soak entry gates

Before beginning the 48-hour run, verify all of the following:

- the dissemination tranche is promoted and post-promotion CI is green;
- four unique data directories are configured and initially clean;
- every miner uses an intentionally selected valid reward address;
- the seed has a stable LAN address and an explicit TCP port-forward;
- Windows Firewall permits the selected P2P TCP port on the seed;
- every node uses the same Vision-Core commit, protocol identity, and genesis;
- seed reachability is tested from the actual network path each node will use;
- the router's NAT-loopback behavior is known if public-address hairpinning is
  part of the test;
- HTTP APIs remain local or protected;
- logs, `/status` snapshots, process exits, chain height, tip hash, cumulative
  work, peer counts, recovery state, and mining state are collected throughout
  the run;
- planned restart, seed outage, peer churn, and reconnection checkpoints have
  explicit expected outcomes.

## Characterization verdict

Do not start the all-miner 48-hour soak on the current runtime. First review and
implement block dissemination as an isolated networking tranche. A shorter
single-miner dry run may still be useful after the exact topology and NAT path
are verified, because every non-mining leaf can pull the seed's higher-work
chain through its configured outbound seed session.

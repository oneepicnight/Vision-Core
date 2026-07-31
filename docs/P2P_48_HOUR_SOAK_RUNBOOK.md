# Four-Node Internet Soak Runbook

## Purpose and entry boundary

This runbook defines the controlled 48-hour Vision-Core rehearsal using two
laptops and two virtual machines. All four nodes mine independently. One node
is the initial seed only; it does not coordinate mining or decide which chain
wins. Valid competing work remains subject to ordinary cumulative-work fork
choice.

Run this procedure only after the P2P dissemination candidate has completed
review, remote CI, promotion to `main`, and post-promotion CI. Every participant
must run the same promoted commit.

## What the runtime provides

- stable local P2P port selection with `VISION_P2P_PORT=auto`;
- distinct per-process P2P node identities;
- bounded peer addresses shared during handshakes and dynamic dialing of newly
  learned addresses;
- canonical block announcement, request, validation, and relay across live
  peer sessions;
- stale mining-job cancellation when a peer block advances the canonical tip;
- immediate removal of disconnected inbound sessions from peer health, without
  letting an old disconnect invalidate a newer connection;
- ordinary block acceptance for every received block.

Peer exchange occurs during handshakes. It is not continuous peer-list gossip.
Block relay means a connected star remains functional even when every leaf
cannot accept inbound internet connections.

## Network topology

Use one laptop as the seed. Give it a stable LAN address through a router DHCP
reservation. Allow its resolved Vision P2P TCP port through Windows Firewall
and forward that same external TCP port to the seed laptop.

The second laptop on the same router should dial the seed's LAN address. It
should not rely on the router's public address unless NAT loopback has been
explicitly verified. Remote virtual machines should dial the router's public
IPv4 address and forwarded seed port. Virtual machines behind the same router
are local participants, not independent internet paths.

Only the seed requires inbound public reachability for this rehearsal. Other
nodes may advertise a complete reachable identity if one truly exists, but
must not advertise a private address to remote peers as if it were publicly
routable.

Do not forward the HTTP API. It has no built-in TLS or authentication. Vision
Desktop should reach its local Core instance over a protected local path.

## Preflight inventory

Record these values before starting:

| Item | Seed laptop | Laptop 2 | VM 1 | VM 2 |
| --- | --- | --- | --- | --- |
| Host name | | | | |
| Promoted commit | | | | |
| LAN/private IP | | | | |
| Public IPv4/upstream | | | | |
| Resolved P2P port | | | | |
| HTTP port | | | | |
| Data directory | | | | |
| Miner reward address | | | | |
| Seed address used | none | | | |

Each data directory must be unique, empty for a fresh rehearsal, and located on
reliable storage. Each miner must use an intentionally selected valid reward
address. Do not use the default zero address.

## Seed configuration

Set the following on the seed laptop, replacing placeholders:

```powershell
$env:VISION_DATA_DIR = "C:\VisionSoak\seed"
$env:VISION_HTTP_PORT = "17070"
$env:VISION_P2P_PORT = "auto"
$env:VISION_P2P_ADVERTISED_HOST = "<router-public-ipv4>"
$env:VISION_P2P_ADVERTISED_PORT = "auto"
$env:VISION_ALLOW_PRIVATE_PEERS = "true"
$env:VISION_SEED_PEERS = ""
$env:VISION_MINING = "true"
$env:VISION_MINER_ADDRESS = "<seed-valid-64-lowercase-hex-address>"
$env:RUST_LOG = "vision_core=info"
```

Start the seed once and read the resolved P2P port from the startup banner.
Stop it, create the matching TCP firewall allowance and router forward, then
start it again. Confirm the same IP produces the same automatic port. If the
machine's routed IP changes, recheck the derived port and forwarding rule.

## Other participant configuration

Use a unique data directory, HTTP port, and reward address on each participant.
The automatic P2P port may be used independently on every machine.

For the second laptop on the seed's router:

```powershell
$env:VISION_DATA_DIR = "C:\VisionSoak\laptop2"
$env:VISION_HTTP_PORT = "17071"
$env:VISION_P2P_PORT = "auto"
$env:VISION_SEED_PEERS = "<seed-lan-ip>:<seed-resolved-port>"
$env:VISION_ALLOW_PRIVATE_PEERS = "true"
$env:VISION_MINING = "true"
$env:VISION_MINER_ADDRESS = "<unique-valid-64-lowercase-hex-address>"
$env:RUST_LOG = "vision_core=info"
```

For each genuinely remote VM, use the seed's public address:

```powershell
$env:VISION_DATA_DIR = "C:\VisionSoak\vm1"
$env:VISION_HTTP_PORT = "17072"
$env:VISION_P2P_PORT = "auto"
$env:VISION_SEED_PEERS = "<router-public-ipv4>:<seed-resolved-port>"
$env:VISION_ALLOW_PRIVATE_PEERS = "true"
$env:VISION_MINING = "true"
$env:VISION_MINER_ADDRESS = "<unique-valid-64-lowercase-hex-address>"
$env:RUST_LOG = "vision_core=info"
```

Do not set an advertised host or port on a participant that cannot accept
connections at that address. If a participant is intentionally reachable,
configure both fields and ensure the external port maps to its resolved local
port.

## Dry-run gates

Before the 48-hour clock starts, run a 30-minute dry run and verify:

1. all four startup banners show the intended commit, data directory, and
   resolved ports;
2. all four `/status` responses report at least one active peer session;
3. mining becomes active on all four nodes;
4. a block mined by each participant appears at the other three nodes;
5. canonical tip hash, height, and cumulative work converge after normal fork
   races;
6. stopping one non-seed node does not stop the other miners;
7. restarting that node from its existing data directory catches up without
   deleting or replacing `chain.db`;
8. no node reports persistent block rejection, recovery loops, or listener
   failures.

Do not begin the soak if any node remains on a different canonical tip after a
reasonable synchronization interval or if a participant's mined block never
reaches the seed.

## 48-hour observation schedule

Capture `/status`, the process state, and logs from every node at startup; at
15, 30, and 60 minutes; every four hours; immediately before and after each
planned disruption; and at completion. Record at minimum:

- canonical height and tip hash;
- cached state-root height and value;
- active inbound and outbound session counts;
- durable, transient, and dialable peer counts;
- mining active state and blocks found;
- recovery state and reason;
- process uptime, CPU, memory, and disk use;
- warnings, disconnects, rejected blocks, and reconnect attempts.

Structured logs are the durable evidence. The current `/peers` endpoint is not
wired to live peer state, so use `/status` peer counters and logs instead.

## Planned resilience events

After at least six stable hours:

1. stop one non-seed miner for ten minutes, restart it with the same data, and
   verify catch-up and convergence;
2. interrupt the seed's network path for ten minutes while leaving the process
   running, restore it, and verify sessions and tips recover;
3. restart the seed with its existing data directory and verify all nodes
   reconnect and converge;
4. let a normal short fork race occur; do not manually copy databases or force
   a tip.

Abort the rehearsal if two healthy connected groups remain on different tips
without convergence, a node opens an unintended data directory, chain state
cannot survive restart, or repeated accepted blocks fail to relay.

## Completion criteria

The rehearsal passes when all four processes complete the planned duration and
events, all nodes finish on the same canonical tip and cumulative work, every
restart preserves its chain state, blocks from every miner have propagated,
and no unexplained consensus rejection, database error, permanent partition,
or uncontrolled resource growth remains.

Archive the exact commit, environment inventory with secrets excluded, router
mapping, firewall rule, status captures, and complete logs. Environment values
and unknown diagnostic fields are sensitive by default; redact public reports
accordingly.

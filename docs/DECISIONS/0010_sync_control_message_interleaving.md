# ADR-0010: Synchronization Control-Message Interleaving

## Status

Accepted.

## Scope

P2P catch-up and higher-work recovery on an established synchronization
connection.

## Context

Vision-Core uses full-duplex P2P sessions. A peer can legitimately send an
asynchronous `AnnounceBlock`, `Ping`, or `GetHeight` while the synchronization
client has an outstanding `GetBlock` request.

The earlier synchronization client performed one receive after each
`GetBlock` and required that message to be `Block`. During the 2026-08-04
four-node Internet dry run, a mining seed announced a newly accepted block
before returning the older block requested by the recovering node. The
recovering node rejected the valid control message as
`unexpected block reply: AnnounceBlock`, repeatedly restarted higher-work
recovery, and remained at height 1.

This was a message-ordering defect. It was not a block-validity,
cumulative-work, wire-encoding, transport, or persistence failure.

## Decision

An outstanding `GetBlock` request remains active across legitimate
asynchronous control traffic on the same connection:

- `AnnounceBlock` is recorded diagnostically and deferred; it does not satisfy
  or replace the outstanding request;
- `Ping` receives the corresponding `Pong` and the block wait continues;
- `GetHeight` receives the local canonical summary and the block wait
  continues;
- only a `Block` proceeds to the existing requested-hash check and ordinary
  block-import path;
- `Disconnect`, malformed framing, a mismatched block, and other unexpected
  replies continue to fail recovery.

The deferred announcement is not trusted or imported through this path. A
later summary poll can discover work beyond the branch currently being
downloaded. No message variant, framing, protocol version, compatibility
identity, block acceptance rule, or fork-choice rule changes.

## Consequences

- A node can catch up from a peer that continues mining and announcing blocks.
- Sync results no longer depend on whether a valid announcement wins a network
  arrival race with the requested block.
- Peer liveness and height requests remain serviceable during a long branch
  download.
- Every downloaded block still passes the exact requested-hash check and the
  unified block-acceptance path.
- Existing block-request timeout behavior is unchanged and remains outside
  this correction.

## Evidence

- Base commit: `c141bb83cc307fedd6d73ee8fd86af6185799e5d`.
- `b3e8331` adds a deterministic regression that fails on the base with
  `unexpected block reply: AnnounceBlock`.
- `0ba3c43` keeps the outstanding request active across supported control
  traffic.
- `3890e19` extends deterministic coverage across announcement, ping, and
  height traffic before the requested block.
- The observed WAN evidence and candidate validation are recorded in
  [P2P Internet Soak Characterization](../P2P_INTERNET_SOAK_CHARACTERIZATION.md).

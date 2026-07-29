# Vision Project Charter

## Status and Purpose

This charter states why Vision exists and what the project is trying to
preserve over the long term. It is a compass for product, protocol, ecosystem,
and governance decisions.

It is not an engineering procedure. Where an operational rule is required, the
policy documents referenced by repository `AGENTS.md` control.

Technical documentation explains how Vision is built. This charter explains
why it is worth building and what “done right” means.

The charter does not claim that planned capabilities are implemented. Current
engineering reality remains documented in [CURRENT_STATUS.md](CURRENT_STATUS.md),
and technical boundaries remain governed by
[CONSENSUS_BOUNDARIES.md](CONSENSUS_BOUNDARIES.md).

## Why Vision Exists

Vision exists to make digital ownership durable, independent, and directly
usable.

People increasingly spend time, labor, creativity, and money in digital
environments. They acquire identities, assets, land, reputation, and economic
relationships that may be valuable to them. Yet those things usually exist
only by permission of a company. A publisher can close a game, a marketplace
can remove an account, a platform can change its rules, and a service can
disappear. When it does, the user’s history and property can disappear with it.

Vision seeks to establish a blockchain foundation where ownership and
transaction history do not depend on the continued existence or goodwill of a
single operator.

The protocol comes first. Applications come second.

Gaming is the flagship application because it brings digital ownership,
identity, exchange, creativity, land, and persistent worlds together in a form
people immediately understand. Vision is not itself a game. It is intended to
be the durable foundation on which games and other digital economies can be
built.

## The Problems Vision Intends to Solve

### Conditional digital ownership

Most digital ownership is contractual access controlled by a platform rather
than possession supported by an independent record. Vision aims to make
ownership verifiable outside the application that first issued or displayed
the asset.

### Platform dependence

Digital economies are commonly tied to one company’s servers, identity system,
marketplace, and payment rules. Vision aims to reduce that dependency by
providing an open settlement and ownership layer.

### Closed exchange

Users are often unable to exchange digital property directly, or may do so only
through a mandatory intermediary. Vision aims to support peer-to-peer exchange
while allowing applications to build discovery, safety, and user experience on
top.

### Fragile digital history

Virtual worlds and creator economies can outlive individual products, but their
records rarely can. Vision aims to preserve independently verifiable ownership
and transaction history for as long as the network remains viable.

### Fragmented creator participation

Creators contribute art, code, environments, experiences, and communities but
often lack durable attribution or direct economic relationships with users.
Vision aims to provide a foundation on which transparent creator economies can
be designed.

### Identity confined to applications

Digital identity and reputation are usually reset at each platform boundary.
Vision’s long-term direction includes user-controlled identity that applications
may recognize without any one application owning the person’s entire digital
existence.

## Enduring Principles

### Decentralization must remain meaningful

Vision should not use decentralization as a label while placing practical
control in a hidden central service. Convenience layers may exist, but the
ability to verify ownership and protocol state must not depend on one company.

### Users should control their property

The project should favor architectures in which users can hold keys, authorize
transfers, verify state, and move between compatible applications. Custodial
services may improve accessibility, but they must not redefine ownership.

### Consensus integrity is not a product tradeoff

No launch date, partnership, feature, or short-term market opportunity justifies
concealing a consensus risk or weakening historical compatibility without a
deliberate protocol decision.

### History should remain honest

Published tags, releases, decisions, and compatibility behavior are part of the
project’s institutional integrity. History must not be rewritten to make the
current story more convenient.

### The protocol must precede platform power

Vision should build open protocol capability before using applications to
create dependency. Desktop, wallet, exchange, marketplace, gaming, and creator
services consume the blockchain; they do not become alternate consensus
authorities.

### Permanence requires maintainability

Long-lived ownership cannot rest on software that only its original creators
understand. Documentation, deterministic testing, reproducible releases,
reviewable commits, and explicit decisions are part of the product’s durability.

### Security and correctness outweigh speed

A delayed feature can be delivered later. Corrupted state, incompatible nodes,
or compromised ownership may not be repairable. Vision should choose the safer
review and validation path when consequences are uncertain.

### Planned capability must not be presented as current fact

Vision should distinguish implemented, validated, approved, planned, and
visionary work. Credibility depends on describing the project as it is while
building toward what it can become.

### Open participation requires clear boundaries

Contributors should be able to understand how to propose, implement, validate,
and review work without relying on private history. Protocol authority,
release authority, application responsibility, and unresolved owner decisions
must remain explicit.

## What Must Never Be Sacrificed

Vision must not sacrifice:

- deterministic consensus for implementation convenience;
- historical validity for cosmetic simplification;
- user ownership for platform lock-in;
- verifiability for opaque operational control;
- security for release speed;
- honest release identity for a cleaner narrative;
- protocol neutrality for one application’s short-term advantage;
- maintainability for rapid accumulation of features;
- explicit consent for hidden custody;
- evidence-based engineering for assumption or urgency.

These principles may make development slower in the short term. They are what
allow the project to remain credible over the long term.

## The Five-to-Ten-Year Direction

The following describes success, not a guaranteed schedule.

Within five to ten years, Vision should aim to be a dependable blockchain
foundation that independent operators, developers, creators, and users can
participate in without requiring permission from a single company.

Success would include:

- a secure, deterministic, independently operated core network;
- durable ownership and provenance for digital assets;
- wallets that give users understandable control of keys and transactions;
- peer-to-peer exchange with clear settlement and custody boundaries;
- marketplaces whose discovery services do not become ownership authorities;
- developer tools that make correct integration easier than protocol
  reinvention;
- games that recognize blockchain-backed assets, identity, and economic
  history;
- creator economies with transparent attribution and payment rules;
- carefully designed virtual-land ownership and governance;
- persistent online worlds whose ownership history can survive a client,
  studio, marketplace, or hosting provider;
- a protocol and repository that can be maintained by contributors who were
  not present at its creation.

The strongest evidence of success would not be a single flagship product. It
would be multiple independent applications choosing Vision because its
ownership, settlement, and verification guarantees are useful and trustworthy.

## Opportunities Vision Should Pursue

Vision should favor opportunities that:

- strengthen the core blockchain’s correctness, resilience, or usability;
- increase independent node operation and verification;
- give users more direct control of identity, assets, and exchange;
- create open interfaces usable by more than one application;
- improve wallet safety and comprehension;
- make peer-to-peer exchange practical without making custody mandatory;
- preserve creator attribution and enable direct economic participation;
- connect games and persistent worlds to durable ownership;
- improve interoperability without weakening consensus;
- expand the contributor and operator community;
- create sustainable funding aligned with protocol health and user ownership;
- produce reusable infrastructure rather than one-off demonstrations.

Partnerships and products are most aligned when they create capability that
remains valuable even if the original partner or application later leaves.

## Opportunities Vision Should Avoid

Vision should intentionally avoid opportunities that:

- require hidden central control while being marketed as decentralized;
- make one game, marketplace, wallet, or company the practical owner of the
  protocol;
- trade historical compatibility for a short-term demonstration;
- introduce custody without explicit user understanding and security design;
- place private keys or consensus authority in unnecessary cloud services;
- treat speculative price activity as the project’s primary purpose;
- promise protocol capabilities before they are implemented and validated;
- create incompatible application-specific consensus rules;
- require rushed consensus, genesis, serialization, persistence, or VisionX
  changes;
- make ownership dependent on proprietary indexes or APIs;
- accumulate integrations that the project cannot securely maintain;
- obscure fees, issuance, governance, or administrator privileges;
- use irreversible blockchain commitments for problems that do not benefit
  from permanence.

Not every commercially attractive feature belongs in the protocol. Vision
should decline work that increases adoption metrics while weakening the reason
the network exists.

## What “Done Right” Means

Vision is being done right when:

- users can verify and control what they own;
- independent nodes reach the same result from the same valid history;
- historical chain data remains valid under documented rules;
- applications can fail without erasing blockchain ownership;
- product layers consume protocol services without duplicating consensus;
- releases are reproducible, immutable, and supported by exact evidence;
- protocol changes are deliberate, isolated, compatible, and governed;
- persistence survives restart, recovery, and supported upgrades;
- security limitations are disclosed rather than concealed;
- contributors can understand current state and safe workflows from the
  repository;
- roadmap language remains distinct from implemented capability;
- the project can outlive its original contributors.

“Done right” does not mean finished. A durable protocol and ecosystem will
continue to evolve. It means each stage leaves Vision more trustworthy,
understandable, independently usable, and faithful to its purpose.

## Decision Compass

When evaluating a major decision, ask:

1. Does this increase or reduce user control?
2. Does it strengthen or weaken independent verification?
3. Does it create an open capability or a new dependency?
4. Does it preserve consensus and historical compatibility?
5. Can the project maintain and secure it?
6. Is the protocol the correct layer for it?
7. Would the decision still benefit Vision if the sponsoring application or
   partner disappeared?
8. Are risks, authority, fees, custody, and governance visible to users?
9. Is the proposal supported by evidence and an achievable validation plan?
10. Will future contributors understand why the decision was made?

If a proposal conflicts with the enduring principles, the default answer is
no. If the evidence is incomplete, investigate before committing the project.

## Stewardship

This charter is living documentation, but it should not change merely to fit a
temporary feature or commercial opportunity. Material changes should be
reviewed as project-governance decisions and should explain which principle,
mission, or long-term objective has changed and why.

New features should update the technical and operational documentation that
they affect. The documentation system should grow because project reality
changes, not because more prose is desired.

With this charter and the Project Intelligence Layer in place, the
documentation phase is considered complete. The repository’s focus returns to
reviewing and promoting the Developer Readiness work, then continuing
Configuration Hardening and building the Vision blockchain and ecosystem.

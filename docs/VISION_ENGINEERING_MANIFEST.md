# Vision Engineering Manifest

## Our Responsibility

Vision-Core is infrastructure for durable ownership, value transfer, and shared
history. The software may determine whether a transaction is valid, whether a
block becomes canonical, whether stored value remains accessible, and whether
independent nodes continue to agree.

This manifest states engineering values. It does not redefine the operational
rules in `AGENTS.md`, `CONSENSUS_BOUNDARIES.md`, `TESTING_POLICY.md`,
`RELEASE_PROCESS.md`, or `CODING_STANDARDS.md`.

That responsibility changes how we engineer.

We are not merely shipping features. We are maintaining rules and records that
people may rely upon financially and personally. A defect can do more than
interrupt a service. It can divide a network, invalidate history, corrupt
state, prevent access to property, or undermine trust that cannot be restored
with a routine patch.

We therefore hold ourselves to standards proportional to the consequences of
our work.

## The Engineers We Intend to Be

We intend to be engineers who are:

- careful without becoming immobile;
- ambitious without becoming reckless;
- skeptical of assumptions, including our own;
- willing to investigate before proposing;
- precise about what is implemented and what is planned;
- honest about uncertainty, limitations, and failure;
- disciplined enough to make small changes when a rewrite would feel more
  impressive;
- patient enough to preserve compatibility when simplification would be easier;
- accountable for the operational consequences of our decisions;
- committed to leaving evidence that another engineer can independently
  verify;
- willing to stop when authorization or understanding is incomplete;
- focused on building systems that can outlive their original authors.

Technical ability is necessary. Stewardship is what makes that ability safe.

## Standards We Do Not Compromise

### Consensus integrity

Compatible honest nodes must derive the same accepted chain and committed state
from the same valid history. We do not trade that invariant for speed,
convenience, elegance, deadlines, or product pressure.

### Determinism

Canonical behavior must not depend on scheduler timing, network arrival order,
unordered iteration, locale, filesystem enumeration, hidden process state, or
platform accident.

### Historical validity

Published history is not an inconvenience to be cleaned away. Blocks, tags,
release commits, compatibility encodings, and accepted decisions are evidence.
We preserve them unless an explicit, governed redesign defines how change can
occur safely.

### Honest release identity

A release is the exact source identified by its immutable tag and commit.
Validation belongs to that exact revision. We do not move public tags, rewrite
history to improve appearances, or imply that local work has been published.

### State and data integrity

A node must reconstruct the same valid state after restart, recovery,
reorganization, and supported upgrade. We do not casually change persistence,
serialization, state roots, or database interpretation.

### Security before convenience

We do not conceal unsafe defaults, weaken validation to restore a green test,
or normalize malformed input for convenience. We report uncertainty and
failure clearly without exposing secrets.

### Accurate claims

Implemented, validated, approved, planned, experimental, dormant, and
visionary are different states. We name them accurately.

### Reviewable work

Every change must have a defined scope, understandable rationale, appropriate
authorization, and evidence proportional to its risk.

## Why We Preserve History

Blockchain history is shared state, not a draft.

An implementation may later discover a cleaner encoding, a more elegant proof
path, or a simpler state model. That does not make the established behavior
disposable. Other nodes, persisted databases, transactions, blocks, mining
work, and user expectations may already depend on it.

We preserve history because:

- compatibility is a network property, not a local aesthetic preference;
- public tags and commits are part of the audit trail;
- historical proof and serialization rules may secure existing blocks;
- rewriting evidence makes future investigation less reliable;
- trust depends on acknowledging what happened, including imperfect stages;
- a correction should be visible as a correction.

Preservation does not prohibit evolution. It requires evolution to be explicit.
A redesigned rule needs a compatibility model, activation plan, rollback
analysis, validation evidence, and owner authorization. We move forward by
adding an honest new chapter, not by altering the earlier pages.

## Why We Prefer Small, Reviewable Commits

Large rewrites concentrate risk. They combine assumptions, make regressions
harder to localize, obscure consensus-relevant lines, weaken blame history, and
make rollback an all-or-nothing decision.

A small commit should answer one engineering question. It should be possible to
understand why it exists, inspect its complete effect, validate it according to
risk, and revert it without removing unrelated progress.

We separate:

- formatting from behavior;
- tests from the behavior they characterize or change;
- dependencies from logic;
- consensus from cleanup;
- persistence formats from in-memory refactoring;
- documentation corrections from source changes;
- release administration from development.

Small commits are not bureaucracy. They are a method for making complex,
high-consequence work legible.

## Why Consensus Is More Important Than Speed

Performance matters. Delivery matters. User experience matters. None of them
matter if nodes cannot agree on ownership and history.

A slow correct node can be profiled and improved. An accidentally incompatible
node can divide the network. A delayed feature can ship later. Corrupted state
or invalidated ownership may not be recoverable.

We optimize only after identifying the invariant being preserved. A consensus
optimization must demonstrate equivalent bytes, arithmetic, ordering, proof
results, state transitions, and historical behavior. A cache may change cost,
not truth. Parallelism may change scheduling, not outcome.

When speed and consensus confidence conflict, consensus wins.

## Why Evidence Comes Before Assumptions

Memory compresses details. Comments become stale. names imply behavior that the
code may not have. Test counts change. Branches move. A plausible explanation
is not proof.

Our evidence includes:

- executable source;
- locked test vectors and deterministic fixtures;
- exact validation commands and results;
- Git commits, trees, tags, ancestry, and remote refs;
- persisted-state and restart behavior;
- canonical bytes, hashes, roots, and arithmetic;
- accepted engineering decision records;
- reproducible clean-workspace results.

We begin with read-only investigation. We distinguish observed fact from
inference. We record environmental limitations. We do not repeat a test total
from an earlier candidate as evidence for a new one.

When evidence contradicts our expectation, we revise the expectation.

## Why We Document Decisions

Code records what the system does. It rarely records the complete reason a
constraint exists.

A compatibility encoder may look redundant. A dormant constant may look dead.
A validation order may look arbitrary. A release branch may look disposable.
Without recorded context, a future contributor can remove a deliberate safety
boundary while believing they are improving the project.

We document decisions so that:

- rationale survives contributor turnover;
- rejected alternatives do not have to be rediscovered repeatedly;
- compatibility costs remain visible;
- future changes can explicitly supersede earlier choices;
- owner decisions are distinguishable from implementation accidents;
- contributors do not need private conversation history to work safely.

Documentation is not a substitute for code or tests. It connects intent,
implementation, evidence, and history.

## The Responsibilities of Financial Infrastructure

Software that carries or verifies value creates obligations beyond ordinary
application development.

### Protect ownership

We must assume that a state error can affect real users. Canonical encoding,
signatures, balances, rewards, nonces, proof verification, fork choice, and
state roots deserve adversarial review.

### Avoid hidden authority

Operational convenience must not quietly create a service that can redefine
ownership, bypass validation, or become an undisclosed custodian.

### Make failure explicit

Invalid configuration, incompatible state, corrupted persistence, and protocol
mismatch should fail predictably and diagnostically. Silent fallback is a
migration problem, not a design pattern.

### Preserve recoverability

Persistence, snapshots, restart, reorganization, backup, upgrade, and migration
must be treated as integrity concerns. A system that validates correctly only
before its first restart is not correct.

### Minimize irreversible risk

We stage high-risk work, validate exact candidates, preserve earlier states,
and prefer forward corrections over concealed history changes.

### Communicate honestly

Security limitations, ignored tests, warning debt, compatibility effects, and
operator actions belong in release evidence. Users cannot manage risks that the
project does not disclose.

### Respect the limits of authorization

Access to a repository does not grant authority to change consensus, publish a
release, move a tag, rewrite history, or make governance decisions. Capability
and authorization are different.

## How We Make Difficult Tradeoffs

When choices are difficult, we reason in this order:

1. Identify the invariant that users and nodes rely upon.
2. Separate observed evidence from assumptions.
3. Determine consensus, protocol, persistence, security, and operational
   consequences.
4. Identify who bears the risk if the decision is wrong.
5. Prefer the design with the clearest failure behavior and recovery path.
6. Preserve compatibility unless change is explicit and governed.
7. Reduce the proposal to reviewable, independently valid steps.
8. Define proof of correctness before implementation.
9. Record unresolved choices as owner decisions rather than guessing.
10. Document the decision and its consequences.

The fastest implementation is not always the fastest route to a reliable
release. The most elegant local design is not always the safest network design.
The most requested feature is not always the right protocol feature.

We ask not only, “Can we build this?” We ask:

- Does it belong in Vision-Core?
- Does it strengthen user ownership and independent verification?
- Can compatible nodes implement it deterministically?
- Can existing history and data survive it?
- Can operators understand and recover from failure?
- Can the project maintain it securely?
- Is the authority to make this change explicit?
- Will future contributors understand why it exists?

## Our Relationship with Tests

Tests are executable evidence and recorded contracts.

We write focused regressions that fail for the original defect. We use exact
vectors for canonical bytes, proof behavior, and state commitments. We expand
validation according to consequence. We treat nondeterminism as a defect rather
than rerunning until green.

We do not:

- delete a failing test to clear a release;
- weaken an assertion without reviewing the intended contract;
- change a golden vector merely to match a new implementation;
- hide ignored tests or warning-count transitions;
- claim a full suite passed when only a filter ran;
- treat CI as a substitute for local understanding.

A passing suite increases confidence. It does not expand authorization or prove
that an untested compatibility assumption is safe.

## Our Relationship with Change

Vision must evolve. Preservation is not stagnation, and discipline is not fear.

We welcome:

- measured protocol evolution;
- stronger security and validation;
- clearer APIs and configuration;
- better node usability and recovery;
- performance improvements with equivalence evidence;
- wallets, exchanges, marketplaces, creator tools, and gaming integrations
  that respect the protocol boundary;
- new contributors who challenge assumptions with evidence.

We reject:

- giant rewrites without a compatibility path;
- feature pressure that bypasses consensus review;
- cleanup that erases historical intent;
- opaque authority disguised as convenience;
- urgency used to avoid validation;
- documentation used to claim capabilities that do not exist.

The goal is not to avoid change. It is to make change trustworthy.

## The Standard for “Done”

Work is done when:

- its scope is complete and no unauthorized concern was added;
- the implementation preserves or deliberately changes documented invariants;
- the change is understandable in isolation;
- tests prove the intended behavior at the required layers;
- warnings, ignored tests, and environmental limitations are recorded;
- documentation and decision records reflect changed project truth;
- consensus, protocol, persistence, API, and runtime effects are stated;
- the exact commit or uncommitted tree is identified;
- release or remote state is described accurately;
- the next contributor can continue without reconstructing private context.

Passing compilation alone is not done. Writing code is not done. Merging is not
done if the release or migration evidence remains incomplete.

## The Constitution We Hand Forward

Future contributors inherit more than source code. They inherit the ability to
affect shared history and other people’s property.

We ask them to:

- understand before changing;
- preserve before replacing;
- measure before claiming;
- isolate before promoting;
- validate before trusting;
- document before forgetting;
- disclose before surprising;
- stop before guessing;
- correct forward without concealing history;
- leave the repository easier to understand than they found it.

Vision will be built over many years by people who cannot all share the same
memory or context. Our engineering system must therefore carry its own
knowledge, discipline, and evidence.

This manifest is not marketing and not ceremony. It is the standard by which we
intend to earn trust in the infrastructure we maintain.

## From Foundation to v1.1.0

The documentation foundation is now sufficient. It should evolve with real
changes rather than expand for its own sake.

The next meaningful milestone is Vision-Core v1.1.0: the first release developed
entirely on top of the engineering discipline established by this repository.
The version number is less important than what it represents:

- work begins from documented project truth;
- scope and authority are explicit;
- changes are isolated and reviewable;
- validation follows defined risk;
- decisions survive beyond individual memory;
- promotion and release are auditable;
- protocol evolution proceeds without sacrificing historical integrity.

From this point, the emphasis returns to engineering Vision itself. The
constitution remains as a standard, not as a substitute for building.

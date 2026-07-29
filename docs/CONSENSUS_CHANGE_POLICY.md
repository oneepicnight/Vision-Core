# Consensus Change Policy

Vision-Core treats compatibility as a release-governance concern. A change that
appears mechanical can still alter serialized bytes, validation decisions, or
historical behavior.

## Sensitive areas

Enhanced review is mandatory for:

- block and header encoding;
- transaction encoding, validation, and execution;
- PoW target comparison and difficulty calculation;
- VisionX parameters, datasets, hashing, and historical compatibility;
- genesis construction and locked hashes;
- chain acceptance and reorganization;
- canonical state-vector and state-root calculation;
- P2P wire messages, framing, handshake identity, and compatibility versions;
- persisted consensus data or recovery interpretation.

## Commit isolation

One consensus behavior change is permitted per commit. Do not combine it with:

- repository-wide formatting;
- warning cleanup;
- renames unrelated to the behavior;
- dependency upgrades;
- documentation encoding repair;
- another consensus or protocol change.

The commit message and review description must state the rule being changed,
activation or compatibility expectations, and the exact validation evidence.

## Required evidence

A consensus-sensitive change requires:

1. Characterization tests for the behavior before modification.
2. Golden vectors for every affected serialization, digest, root, or validation
   boundary.
3. Historical compatibility review against published releases and tags.
4. Serialization review confirming whether bytes change.
5. Full release validation in release mode with one test thread.
6. Focused tests for the affected module.
7. Cross-machine validation when platform behavior, concurrency, byte order,
   filesystem behavior, or toolchain code generation could matter.

Cross-machine evidence should identify operating system, architecture, Rust
toolchain, commit, command, and result.

## Release suite

The baseline command is:

```powershell
cargo test --release --locked -- --test-threads=1
```

Additional watchdog and VisionX commands are documented in the README.
Consensus vectors must never be updated merely to make a changed
implementation pass; the review must establish why any new vector is correct.

## History and release rules

- Never rewrite published release history.
- Never move or replace an existing release tag.
- Preserve compatibility evidence for historical tags.
- Use an isolated branch and normal review/promotion.
- Do not describe a behavior change as formatting, cleanup, or refactoring.
- Do not combine formatting with a behavioral consensus change.

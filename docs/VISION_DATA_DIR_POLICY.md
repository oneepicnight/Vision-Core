# Vision Data Directory Policy

## Status

Approved policy for Configuration Hardening Tranche 4.

Tranche 4A records and tests the current behavior. It does not implement this
policy. Tranche 4B requires separate authorization before changing runtime
behavior.

## Scope

`VISION_DATA_DIR` selects the directory under which Vision-Core opens
`chain.db`. That database contains durable chain state and snapshots. The
setting is therefore persistence-sensitive even though it does not redefine
consensus rules or stored encodings.

An explicitly supplied but invalid `VISION_DATA_DIR` must never fall back to
the default location.

## Current Behavior Characterized by Tranche 4A

- A missing value becomes `./data`.
- Every present string is preserved verbatim, including an empty string,
  whitespace-only input, surrounding whitespace, and relative paths.
- `node::bootstrap::initialize_chain_state` passes the string to
  `ChainState::open_with_genesis`.
- `ChainState::open_with_genesis` asks sled to open
  `<data-directory>/chain.db`.
- A missing directory is created by the database-opening path when its parent
  and permissions permit.
- An existing directory is used and receives or reopens `chain.db`.
- A regular file or another unusable parent produces a database-open error.
- No dedicated configuration error, path diagnostic, normalization,
  canonicalization, or pre-open validation currently exists.
- The process working directory supplies normal operating-system resolution
  for relative paths.
- Permission-denied behavior is delegated to the filesystem and sled. A
  portable deterministic permission-denied test is not currently available
  because supported non-Windows platforms remain an Owner Decision Required
  item and Windows access-control behavior depends on the executing account.

These statements describe existing behavior; they do not endorse it.

## Approved Tranche 4B Policy

### Missing input

When `VISION_DATA_DIR` is missing, preserve the existing default exactly:
`./data`.

### Explicit input

Reject:

- an empty value;
- a whitespace-only value;
- a value with leading or trailing whitespace;
- a path that points to an existing regular file;
- a path that cannot be created;
- a path that cannot be accessed with the permissions required to open the
  database;
- a path that cannot produce a usable `chain.db` location.

Do not trim or otherwise reinterpret explicit input. The error must identify
`VISION_DATA_DIR`, the rejected non-secret value, and the failed requirement or
filesystem operation.

### Relative paths

Continue to accept relative paths for compatibility. Resolve them against the
process working directory and report the resulting effective location before
opening `chain.db`.

Full filesystem canonicalization must not be required before creation because
the configured directory may not exist. Tranche 4B may perform only
lexical/absolute resolution that does not change path semantics.

### Directory creation and existing state

Allow creation of the configured directory when its parent exists and
permissions permit.

When an existing database is selected:

- never delete it;
- never replace it;
- never migrate it under this tranche;
- never silently select another directory;
- preserve the existing `chain.db` layout and every stored encoding.

### Startup ordering and failure

Validate the effective path before database initialization. Report the
effective data directory clearly before opening `chain.db`.

Return a structured startup error for invalid or unusable input. Do not panic,
fall back to another directory, or continue starting services.

## Tranche 4B Test Design

The implementation tranche must include deterministic tests for:

- missing input retaining `./data`;
- empty, whitespace-only, and padded values being rejected;
- relative paths resolving from a controlled process working directory;
- existing and nonexistent directories selecting the intended `chain.db`;
- a regular-file path being rejected without modifying the file;
- creation failure and permission failure where the platform can express them
  deterministically;
- an existing database reopening without replacement or migration;
- restart preserving the selected tip and state;
- snapshots reopening from the same database;
- reorganization state surviving restart;
- state-root equality before and after restart;
- errors occurring before storage initialization and service startup;
- no fallback database being created after explicit invalid input.

Run storage, restart, reorganization, snapshot, state-root, full release, and
CI validation against the exact Tranche 4B candidate.

## Non-Goals

This policy does not authorize:

- database-format or snapshot-format changes;
- migrations;
- automatic recovery by deleting or replacing state;
- network-specific directory layouts;
- new configuration sources or precedence;
- runtime configuration reload;
- changes to consensus, protocol, serialization, or state-root calculation.

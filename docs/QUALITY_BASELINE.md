# Formatting, Compiler Warning, and Clippy Baseline

This document records the developer-quality baseline established in Tranche 2
on the currently validated Rust 1.97.1 toolchain.

## Policy

- `cargo fmt --all -- --check` must pass. Formatting is a blocking CI gate.
- `cargo check --all-targets --locked` must complete successfully.
- `cargo test --release --locked -- --test-threads=1` must complete with only
  the documented ignored test.
- `cargo clippy --all-targets --locked` must complete successfully, but remains
  a non-blocking CI debt report until the findings below are classified and
  resolved.
- New or modified code must not add compiler or Clippy warnings.
- Do not suppress a repository warning merely to improve the count.
- Do not delete a dead item until the Tranche 3 dead-code ledger classifies it.
- Consensus-sensitive lint changes require the same isolation and validation as
  any other consensus-sensitive edit.

Use `--offline` with these commands when the declared toolchain and locked
dependencies are already cached.

## Established results

The post-cleanup Tranche 2 commands report:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Clean |
| `cargo check --all-targets --locked --offline` | Success |
| Normal binary compiler warnings | 58 |
| Test binary compiler warnings | 34, including 25 also emitted for the normal binary |
| `cargo clippy --all-targets --locked --offline` | Success |
| Normal binary Clippy/compiler warnings | 70 |
| Test binary Clippy/compiler warnings | 71, including 37 also emitted for the normal binary |

These are Cargo target summaries, not a count of warning kinds. The compiler
baseline therefore contains 67 non-duplicated diagnostic emissions across the
two targets, and the Clippy run contains 104.

Tranche 2 removed unambiguous unused imports, unnecessary test mutability, a
deprecated `TempDir::into_path` call, and simple test-only Clippy findings. It
did not remove public re-exports or dead code.

## Remaining compiler-warning classes

The compiler baseline consists of:

- public module re-exports unused by the current binary;
- unused functions, methods, fields, constants, structs, and enum variants;
- test-support items that are not invoked by the current test selection.

These findings cross public API, test support, configuration, P2P, mining, PoW,
VisionX, genesis, persistence, and chain-acceptance boundaries. They are inputs
to Tranche 3 and are not evidence that an item is safe to delete.

## Remaining Clippy classes

In addition to the compiler warnings, Clippy reports mechanical suggestions
such as operand references, sorting forms, range loops, clamps, argument count,
and test expressions. Several occur in consensus-sensitive modules. They remain
unchanged so a formatting/warning tranche cannot silently alter consensus,
serialization, PoW, VisionX, chain-state, or P2P behavior.

## Comparing future changes

Run the commands above on the branch base and on the proposed change using the
same Rust toolchain. A pull request should identify:

1. any warning added, removed, or changed;
2. why the change is behavior-preserving;
3. whether a public or dormant API was involved;
4. the focused and full validation results.

The warning counts may decrease only through reviewed changes. A lower count is
not, by itself, authorization to remove code.

The Tranche 3 classification and deletion prerequisites are maintained in the
[dead-code classification ledger](DEAD_CODE_LEDGER.md).

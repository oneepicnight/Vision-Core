# Formatting, Compiler Warning, and Clippy Baseline

This document records the developer-quality baseline established in Tranche 2
on the currently validated Rust 1.97.1 toolchain.

This is a historical tranche baseline, not the current warning total. Approved
Tranche 3 cleanup reduced the test-target compiler summary from 34 to 31 while
the normal-target summary remained 58. The reason and current counts are
maintained in [CURRENT_STATUS.md](CURRENT_STATUS.md).

## Policy Authority

This document records historical measurements. Current validation rules are
defined in [TESTING_POLICY.md](TESTING_POLICY.md), coding and warning rules in
[CODING_STANDARDS.md](CODING_STANDARDS.md), and unused-code dispositions in
[DEAD_CODE_LEDGER.md](DEAD_CODE_LEDGER.md).

The commands below were the commands used to establish Tranche 2. They are
evidence, not a second validation policy. `--offline` was used because the
pinned toolchain and locked dependencies were cached.

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

## Comparing later changes

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

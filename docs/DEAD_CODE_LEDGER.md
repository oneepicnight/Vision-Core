# Dead-Code Classification Ledger

This ledger classifies the unused-code findings reported by Rust 1.97.1 after
the Vision-Core v1.0.4 formatting and warning-baseline tranche.

It is an audit, not deletion authorization. No item may be removed solely
because the compiler calls it unused.

## Classification rules

- **Obsolete**: evidence indicates that the item has been superseded or is an
  uncalled test remnant. Deletion still requires a focused review.
- **Near-term planned API**: a public façade or operational interface whose
  supported status has not yet been decided.
- **Test support**: used only from `#[cfg(test)]` code, or clearly intended as a
  test helper.
- **Dormant consensus/protocol feature**: inactive code or data that describes,
  verifies, or could change consensus, economics, PoW, fork choice, or wire
  behavior.
- **Uncertain**: evidence is insufficient to choose safely.

Sensitivity is recorded independently. “Application” does not mean “safe to
delete”; it means the item is outside the currently identified
consensus-critical path.

## Public façade and re-export findings

These warnings arise because Vision-Core is currently a binary crate. A
`pub use` can be unused internally while still expressing an intended module
boundary.

| ID | Symbols | Location | Classification | Sensitivity | Evidence and required decision |
| --- | --- | --- | --- | --- | --- |
| API-01 | `apply_block`, `verify_pow_only` | `src/chain/mod.rs` | Near-term planned API | Consensus-critical | Chain façade exports. Decide whether a supported Rust library API will exist before changing visibility. |
| API-02 | `compute_econ_hash`, `compute_genesis_pow_hash`, `genesis_balances`, `genesis_block`, `validate_econ_hash`, `validate_genesis_hash`, `verify_peer_genesis`, `verify_stored_genesis`, `ECON_HASH` | `src/genesis/mod.rs` | Dormant consensus/protocol feature | Consensus-critical | Genesis and economics commitments include hard-fork warnings and peer/database validation intent. Preserve until the genesis/economics contract is reviewed. |
| API-03 | `MempoolAdmission` | `src/mempool/mod.rs` | Near-term planned API | Consensus-adjacent policy | Public admission façade; library/API status is unresolved. |
| API-04 | `build_candidate`, `MiningJob`, `MiningStats` | `src/miner/mod.rs` | Near-term planned API | Consensus-adjacent mining | Mining façade. Decide external miner/library boundary before removal. |
| API-05 | `P2PMessage`, `PeerManager`, `PeerState` | `src/p2p/mod.rs` | Near-term planned API | Protocol-sensitive | P2P façade and wire-message type. Requires API and wire-compatibility review. |
| API-06 | `calculate_next_difficulty`, `difficulty_to_target`, `expected_block_difficulty`, `verify_pow_hash` | `src/pow/mod.rs` | Dormant consensus/protocol feature | Consensus-critical | Difficulty and target functions are consensus primitives even when not called by the current binary path. |
| API-07 | `historical_vpow_message_bytes`, `historical_vpow_message_bytes_with_nonce_zero` | `src/pow/mod.rs` | Dormant consensus/protocol feature | Historical-consensus compatibility | Explicit historical encoding support with vector tests. Do not remove without a historical compatibility decision. |
| API-08 | `verify`, `VisionXParams`, `PowJob`, `PowSolution` | `src/pow/mod.rs` | Dormant consensus/protocol feature | Consensus-critical PoW | Alternate/direct VisionX verification and mining interfaces. Preserve pending PoW API review. |

## Chain, state, and persistence findings

| ID | Symbols | Location | Classification | Sensitivity | Evidence and disposition |
| --- | --- | --- | --- | --- | --- |
| CHAIN-01 | `AcceptResult::is_accepted` | `src/chain/accept.rs` | Near-term planned API | Consensus-adjacent | Convenience method on the public acceptance result. Retain until the Rust API boundary is decided. |
| CHAIN-02 | `POW_PREVALIDATION_CACHE`, `mark_pow_prevalidated`, `is_pow_prevalidated`, `verify_pow_only` | `src/chain/accept.rs` | Test support | Consensus-critical PoW | The entry point and cache helpers are exercised by acceptance-security tests but not the runtime path. Removal would discard characterization of prevalidation safety; review as one unit. |
| CHAIN-03 | `cumulative_work` | `src/chain/reorg.rs` | Test support | Consensus-critical fork choice | Queried by reorg tests. Keep until tests are rewritten around a supported query or direct state inspection. |
| CHAIN-04 | `cumulative_work_single_block` | `src/chain/reorg.rs` | Obsolete | Test infrastructure | An uncalled test-shaped function. Confirm whether a missing `#[test]` was intentional before deleting or restoring it. |
| CHAIN-05 | `mempool_critical`, `mempool_bulk`, `mempool_ts`, `mempool_height` | `src/chain/state.rs` | Obsolete | Consensus-adjacent state model | Initialized but never read; the active `Mempool` is separate. Before deletion, audit persisted representations, snapshots, reorg recovery, and downstream consumers. |
| CHAIN-06 | `seen_txs` | `src/chain/state.rs` | Uncertain | P2P/transaction policy | Documented as gossip deduplication but unused. Decide whether deduplication belongs in `ChainState`, `Mempool`, or P2P before removal. |
| CHAIN-07 | `ChainState::open` | `src/chain/state.rs` | Test support | Persistence-sensitive | Used only by tests/legacy construction; runtime uses `open_with_genesis`. Retain until startup and recovery construction paths are consolidated. |
| CHAIN-08 | `ChainState::block_at` | `src/chain/state.rs` | Near-term planned API | Chain query | A natural read API with tests. Decide library/API visibility before removal. |
| CHAIN-09 | `ChainState::debit_balance`, `advance_nonce` | `src/chain/state.rs` | Test support | Consensus-critical transaction state | Exercised by unit tests while production transaction execution mutates state through another path. Preserve as characterization helpers until state mutation is centralized. |
| CHAIN-10 | `persist_tip` | `src/chain/storage.rs` | Test support | Persistence-sensitive | Used by storage/API tests. Do not remove without replacing restart and persistence coverage. |

## Configuration and policy constants

| ID | Symbols | Classification | Sensitivity | Evidence and disposition |
| --- | --- | --- | --- | --- |
| CFG-01 | `STALL_DOWNSHIFT_FACTOR` | Dormant consensus/protocol feature | Consensus-critical difficulty | Documentation value mirrors the active integer calculation. Preserve until the float constant is either wired into a non-consensus diagnostic or replaced by a decision record. |
| CFG-02 | `MAX_REORG`, `MAX_REORG_BOOTSTRAP`, `FINALITY_DEPTH` | Dormant consensus/protocol feature | Fork-choice policy | Comments explicitly identify historical/diagnostic semantics. Removal needs a historical fork-choice and operator-diagnostics decision. |
| CFG-03 | `TOKEN_DECIMALS` | Dormant consensus/protocol feature | Consensus economics | Native-unit precision is consensus metadata even though arithmetic currently uses raw units. Preserve until amount-format and API contracts are defined. |
| CFG-04 | `FEE_BURN_BPS` | Dormant consensus/protocol feature | Consensus economics | Declares fee-burning economics not consumed by current execution. Its mismatch with runtime behavior requires an economics decision, not silent deletion. |
| CFG-05 | `RATE_SUBMIT_RPS`, `RATE_GOSSIP_RPS` | Near-term planned API | Application security policy | Declared rate limits are not enforced. Configuration/API hardening should decide whether to implement or remove the claims. |
| CFG-06 | `TARGET_OUTBOUND_PEERS`, `MAX_CONNECTIONS`, `GOSSIP_INTERVAL_SECS` | Near-term planned API | P2P operational policy | Operational P2P controls are declared but unused. Decide service ownership during P2P hardening. |
| CFG-07 | `SYNC_FORK_SEARCH_TIMEOUT_SECS`, `SYNC_FORK_TIMEOUT_SECS`, `SYNC_SHORT_BATCH_TIMEOUT_SECS` | Dormant consensus/protocol feature | Sync/recovery policy | Comments tie these values to fork detection and synchronization fixes. Preserve until the active timeout flow is mapped and tested. |
| CFG-08 | `Settings::mining_threads` | Near-term planned API | Mining configuration | Parsed and documented but not consumed. Configuration hardening must either implement it or remove the setting with migration notes. |

## Genesis and economics findings

| ID | Symbols | Classification | Sensitivity | Evidence and disposition |
| --- | --- | --- | --- | --- |
| GEN-01 | `VAULT_ACCOUNTS`, `compute_econ_hash`, `validate_econ_hash`, `ECON_HASH` | Dormant consensus/protocol feature | Hard-fork economics | Source states that changing the commitment is a hard fork. Runtime does not validate it. Preserve and resolve the intended economic handshake/startup contract. |
| GEN-02 | `verify_peer_genesis` | Dormant consensus/protocol feature | Wire compatibility | Duplicates an intended peer-genesis guard but is not called directly. Compare against active handshake validation before consolidation. |
| GEN-03 | `genesis_balances` | Dormant consensus/protocol feature | Genesis state | Documents the no-premine initial state and has tests. Preserve as an explicit consensus decision until genesis-state construction is centralized. |

## Mempool and mining findings

| ID | Symbols | Classification | Sensitivity | Evidence and disposition |
| --- | --- | --- | --- | --- |
| MEM-01 | `Mempool::insert`, `has`, `is_empty`, `list_ids` | Test support | Transaction policy | Used by unit tests and express basic pool operations; production uses admission/query paths. Retain until the supported mempool interface is defined. |
| MINER-01 | `build_candidate` | Near-term planned API | Mining/consensus-adjacent | Candidate construction helper is exported but unused by the active manager path. Compare behavior with `MinerManager` before consolidation. |
| MINER-02 | `MiningStats::start_time` | Uncertain | Application diagnostics | Captured but unread. Decide whether uptime/hash-rate diagnostics require it. |
| MINER-03 | `MinerManager::current_job` | Near-term planned API | Mining | Natural status/worker interface. Retain until miner API and threading design are settled. |

## Node and P2P findings

| ID | Symbols | Classification | Sensitivity | Evidence and disposition |
| --- | --- | --- | --- | --- |
| NODE-01 | `RecoveryState::mode` | Near-term planned API | Recovery policy | Read accessor for recovery state; likely diagnostic/API boundary. |
| NODE-02 | test helper `block_root` | Obsolete | Test infrastructure | Uncalled helper inside bootstrap tests. Verify no ignored recovery test was intended to use it before deletion. |
| P2P-01 | `PeerState::Connecting` | Near-term planned API | P2P state machine | Declared state not constructed. Decide whether outbound dialing needs an explicit connecting state. |
| P2P-02 | `PeerManager::outbound_count`, `snapshot` | Near-term planned API | P2P diagnostics | Query methods suitable for connection management and API status. |
| P2P-03 | `PeerManager::note_observed_addr` | Test support | Peer identity/security | Covered in tests but unused by runtime. Compare with advertised-identity handling before removal. |
| P2P-04 | `PeerStore` and its methods | Near-term planned API | Peer persistence/security | Complete file-backed component with unit tests, but not wired into services. Decide whether persistent peers are a supported feature. |
| P2P-05 | `PeerStore::is_empty` | Test support | Peer persistence | Method is used only by component tests; decide with `PeerStore` as a unit. |
| P2P-06 | `should_sync` | Test support | Sync/fork choice | Height-only predecessor to active summary/work-aware selection and used by tests. Likely superseded, but preserve until the tests explicitly characterize the replacement. |

## PoW and transaction findings

| ID | Symbols | Classification | Sensitivity | Evidence and disposition |
| --- | --- | --- | --- | --- |
| POW-01 | `verify_pow_hash` | Test support | Consensus-critical PoW | Direct difficulty-target verifier with vector tests; keep as characterization support. |
| POW-02 | `expected_block_difficulty` | Test support | Consensus-critical difficulty | Compatibility/query wrapper exercised by tests. Preserve until difficulty API consolidation. |
| POW-03 | `nonce_from_header`, `meets_target`, `visionx::verify` | Dormant consensus/protocol feature | Consensus-critical VisionX | Private helpers are reachable only through the unused direct `verify` interface. Treat as one dormant verifier, not three independent deletion candidates. |
| POW-04 | `PowJob::new`, `PowSolution::new` | Near-term planned API | Mining/PoW | Constructors for exported mining types. Decide external miner interface before removal. |
| POW-05 | `VisionXMiner::params`, `build_job`, `mine` | Near-term planned API | Consensus-critical PoW | Complete alternate mining interface with tests. Requires equivalence review against active mining before consolidation. |
| TX-01 | `Tx::tx_id` | Test support | Consensus-critical transaction identity | Legacy/convenience identifier used by tests while production prefers `canonical_tx_id`. Removal requires proving both semantics and updating identity vectors deliberately. |
| TEST-01 | `NodeHarness::api_addr` | Obsolete | Test infrastructure | Stored but never read in multi-node tests. Can be considered for a test-only cleanup after confirming no pending API convergence test needs it. |

## Classification summary

| Classification | Ledger entries |
| --- | ---: |
| Obsolete | 4 |
| Near-term planned API | 17 |
| Test support | 12 |
| Dormant consensus/protocol feature | 13 |
| Uncertain | 2 |

Counts are ledger entries, not individual Rust symbols; related symbols are
grouped where they must be reviewed together.

## Recommended sequence

1. Decide whether Vision-Core will expose a supported Rust library API.
2. Decide whether dormant economics, finality, timeout, and VisionX interfaces
   are roadmap commitments or historical records.
3. Separate component tests from runtime APIs using explicit test-support
   modules where behavior is already covered.
4. Review only the four obsolete entries for possible test-only or structural
   removal.
5. Make each accepted removal an isolated commit and rerun the focused tests
   plus the complete single-threaded release suite.

Configuration hardening must handle parsed-but-unused settings and declared but
unenforced policies as behavior changes with migration notes; this ledger does
not authorize those changes.

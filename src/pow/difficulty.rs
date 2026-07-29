use crate::config::constants::*;
use crate::types::Block;

// ─── U256 target type ─────────────────────────────────────────────────────────

/// A PoW target represented as a 32-byte big-endian value.
///
/// Historical Vision PoW only compares the upper 64 bits. The lower 192 bits
/// are wildcard bits for compatibility with `vision-node`.
pub type U256 = [u8; 32];

// ─── Consensus-critical: target ↔ difficulty ─────────────────────────────────

/// Convert a compact u64 difficulty scalar to a U256 target.
///
/// Encoding (matches vision-node `u256_from_difficulty`):
///   `target[0..8]  = (u64::MAX / difficulty).to_be_bytes()`
///   `target[8..32] = [0xFF; 24]`
///
/// Historical compatibility rule:
///   `hash[0..8] <= target[0..8]`
///
/// The lower 192 bits are ignored during validation.
///
/// # Consensus-critical
/// Any change to this encoding changes which hashes are valid on the network.
pub fn difficulty_to_target(difficulty: u64) -> U256 {
    let mut target = [0xFFu8; 32];
    let hi = u64::MAX / difficulty.max(1);
    target[0..8].copy_from_slice(&hi.to_be_bytes());
    target
}

/// Verify that `hash_hex` satisfies `difficulty`.
///
/// Returns `false` if the hex string is malformed or the hash is not 32 bytes.
///
/// # Consensus-critical
/// Historical `vision-node` semantics compare only the upper 64 bits.
pub fn verify_pow_hash(hash_hex: &str, difficulty: u64) -> bool {
    let Ok(hash_bytes) = hex::decode(hash_hex) else {
        return false;
    };
    if hash_bytes.len() != 32 {
        return false;
    }
    let target = difficulty_to_target(difficulty);
    &hash_bytes[0..8] <= &target[0..8]
}

// ─── Consensus-critical: LWMA difficulty adjustment ──────────────────────────

/// Compute the required difficulty for the *next* block given the canonical
/// chain and the current wall-clock time.
///
/// **Algorithm**: Linearly Weighted Moving Average (LWMA) over the last
/// `RETARGET_WINDOW` inter-block intervals. More-recent intervals receive a
/// proportionally higher weight.
///
/// **Interval clamping** (timestamp-manipulation guard):
/// - Each raw interval is clamped to `[LWMA_MIN_INTERVAL_SECS, LWMA_MAX_INTERVAL_SECS]`
///   before entering the weighted sum. This prevents:
///   - A miner back-dating a timestamp to artificially shorten an interval
///     (clamped to min = TARGET_BLOCK_TIME/4).
///   - A miner delaying a block to inflate the average and tank difficulty
///     (clamped to max = TARGET_BLOCK_TIME×6).
///
/// **Wall-clock stall detection**:
/// - If no block has arrived for `STALL_MULTIPLIER × TARGET_BLOCK_TIME` seconds
///   the difficulty is downshifted by 25/100 (integer equivalent of ×0.75) so
///   miners can find the next block and unblock the chain.
///
/// # Consensus-critical
/// Integer-only arithmetic; no floating-point. Must be identical on every node.
pub fn calculate_next_difficulty(blocks: &[Block], now_secs: u64) -> u64 {
    let n = RETARGET_WINDOW as usize;

    // Need at least 2 blocks to form one interval.
    if blocks.len() < 2 {
        return DIFFICULTY_FLOOR;
    }

    // Use the last min(n+1, len) blocks so we get at most n intervals.
    let window: &[Block] = if blocks.len() > n + 1 {
        &blocks[blocks.len() - (n + 1)..]
    } else {
        blocks
    };

    let count = window.len() - 1; // number of intervals (≥ 1)

    // LWMA: weight[i] = i+1 (1-based index within window, oldest first).
    // Most-recent interval has the highest weight.
    let mut weighted_sum: u128 = 0;
    let mut weight_total: u128 = 0;
    let mut sum_difficulty: u128 = 0;

    for i in 0..count {
        let raw_interval = window[i + 1]
            .header
            .timestamp
            .saturating_sub(window[i].header.timestamp);

        // Clamp to [LWMA_MIN_INTERVAL_SECS, LWMA_MAX_INTERVAL_SECS] to prevent
        // timestamp-manipulation attacks (both directions).
        let clamped = raw_interval
            .max(LWMA_MIN_INTERVAL_SECS)
            .min(LWMA_MAX_INTERVAL_SECS);

        let weight = (i + 1) as u128;
        weighted_sum += clamped as u128 * weight;
        weight_total += weight;
        sum_difficulty += window[i + 1].header.difficulty as u128;
    }

    // Weighted average block time observed over the window.
    let lwma_interval = (weighted_sum / weight_total).max(1);
    // Average difficulty of blocks inside the window.
    let avg_difficulty = sum_difficulty / count as u128;

    // Retarget: scale so observed interval converges toward TARGET_BLOCK_TIME.
    //   new_diff = avg_diff × target / lwma
    // If blocks are coming too fast (lwma < target) → new_diff > avg_diff → harder.
    // If blocks are coming too slow (lwma > target) → new_diff < avg_diff → easier.
    let new_diff = avg_difficulty.saturating_mul(TARGET_BLOCK_TIME as u128) / lwma_interval;

    // Wall-clock stall detection: if no block in STALL_MULTIPLIER × TARGET_BLOCK_TIME
    // seconds, apply emergency downshift so miners can find the next block.
    // Integer equivalent of × STALL_DOWNSHIFT_FACTOR (0.75 = 3/4).
    let last_ts = window.last().map(|b| b.header.timestamp).unwrap_or(0);
    let wait_since_last = now_secs.saturating_sub(last_ts);
    let stall_threshold = TARGET_BLOCK_TIME * STALL_MULTIPLIER;

    let new_diff = if wait_since_last > stall_threshold {
        let downshifted = new_diff.saturating_mul(3) / 4; // × 0.75
        tracing::warn!(
            "[DIFFICULTY] Stall detected: {}s > {}s threshold, downshift {} → {}",
            wait_since_last,
            stall_threshold,
            new_diff,
            downshifted
        );
        downshifted
    } else {
        new_diff
    };

    // Floor: must never drop below DIFFICULTY_FLOOR.
    new_diff.max(DIFFICULTY_FLOOR as u128) as u64
}

// ─── Local convenience (non-consensus) ───────────────────────────────────────

/// Return the required difficulty for a candidate block extending `blocks`.
///
/// This is the value that `chain::accept::apply_block` validates the header
/// difficulty against. Delegates entirely to `calculate_next_difficulty`.
pub fn expected_block_difficulty(blocks: &[Block], candidate_ts: u64) -> u64 {
    calculate_next_difficulty(blocks, candidate_ts)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Block, BlockHeader};

    fn make_block(height: u64, timestamp: u64, difficulty: u64) -> Block {
        Block {
            header: BlockHeader {
                parent_hash: "00".repeat(32),
                number: height,
                timestamp,
                difficulty,
                nonce: 0,
                pow_hash: "00".repeat(32),
                state_root: "00".repeat(32),
                tx_root: "0".repeat(64),
                miner: "0xtest".to_string(),
            },
            txs: vec![],
            weight: 0,
        }
    }

    /// Build a chain of `count` blocks each `interval` seconds apart, all with
    /// `difficulty`, starting at height 1 and timestamp `start_ts`.
    fn make_chain(count: usize, interval: u64, difficulty: u64, start_ts: u64) -> Vec<Block> {
        (0..count)
            .map(|i| make_block(i as u64, start_ts + i as u64 * interval, difficulty))
            .collect()
    }

    // ── difficulty_to_target ─────────────────────────────────────────────────

    #[test]
    fn difficulty_to_target_d1_fills_upper_8_bytes() {
        let t = difficulty_to_target(1);
        assert_eq!(&t[0..8], &u64::MAX.to_be_bytes());
        assert_eq!(&t[8..32], &[0xFFu8; 24]);
    }

    #[test]
    fn difficulty_to_target_fills_lower_192_bits_with_ff() {
        let t = difficulty_to_target(2);
        assert_eq!(&t[8..32], &[0xFFu8; 24]);
    }

    #[test]
    fn difficulty_to_target_d2_halves_upper_bytes() {
        let t1 = difficulty_to_target(1);
        let t2 = difficulty_to_target(2);
        let hi1 = u64::from_be_bytes(t1[0..8].try_into().unwrap());
        let hi2 = u64::from_be_bytes(t2[0..8].try_into().unwrap());
        assert_eq!(hi2, hi1 / 2);
    }

    #[test]
    fn difficulty_to_target_higher_difficulty_is_lower_target() {
        let t10 = difficulty_to_target(10);
        let t100 = difficulty_to_target(100);
        // Lower target = harder = higher difficulty.
        assert!(
            t100 < t10,
            "difficulty=100 must have a lower target than difficulty=10"
        );
    }

    #[test]
    fn difficulty_to_target_overflow_safe() {
        // Should not panic at extreme values.
        let _ = difficulty_to_target(u64::MAX);
        let _ = difficulty_to_target(0); // 0 treated as 1
    }

    // ── verify_pow_hash ──────────────────────────────────────────────────────

    #[test]
    fn zero_hash_satisfies_any_difficulty() {
        let zero_hex = "00".repeat(32);
        assert!(verify_pow_hash(&zero_hex, 1));
        assert!(verify_pow_hash(&zero_hex, 1_000_000));
        assert!(verify_pow_hash(&zero_hex, u64::MAX));
    }

    #[test]
    fn all_ff_hash_fails_nontrivial_difficulty() {
        let ff_hex = "ff".repeat(32);
        // Target for difficulty=2: top byte u64::MAX/2 = 0x7fff..., so all-ff hash fails.
        assert!(!verify_pow_hash(&ff_hex, 2));
    }

    #[test]
    fn lower_192_bits_do_not_affect_pow_validity() {
        let low_zero = "7fffffffffffffff000000000000000000000000000000000000000000000000";
        let low_one = "7fffffffffffffff000000000000000000000000000000000000000000000001";
        let low_ff = "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

        assert!(verify_pow_hash(low_zero, 2));
        assert_eq!(verify_pow_hash(low_zero, 2), verify_pow_hash(low_one, 2));
        assert_eq!(verify_pow_hash(low_zero, 2), verify_pow_hash(low_ff, 2));
    }

    #[test]
    fn difficulty_two_uses_documented_upper_64_bit_vectors() {
        let accepted = [
            "7fffffffffffffff000000000000000000000000000000000000000000000000",
            "7fffffffffffffff000000000000000000000000000000000000000000000001",
            "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "7ffffffffffffffeFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
            "0000000000000000FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        ];
        let rejected = [
            "8000000000000000000000000000000000000000000000000000000000000000",
            "8000000000000000FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ];

        for hash in accepted {
            assert!(verify_pow_hash(hash, 2), "hash should be accepted: {hash}");
        }
        for hash in rejected {
            assert!(!verify_pow_hash(hash, 2), "hash should be rejected: {hash}");
        }
    }

    #[test]
    fn verify_pow_hash_rejects_malformed_hex() {
        assert!(!verify_pow_hash("not_hex", 1));
        assert!(!verify_pow_hash("deadbeef", 1)); // too short
    }

    // ── calculate_next_difficulty ────────────────────────────────────────────

    #[test]
    fn floor_clamp_prevents_zero_difficulty() {
        // Only a genesis block — always returns floor.
        let blocks = vec![make_block(0, 0, 1)];
        assert_eq!(calculate_next_difficulty(&blocks, 100), DIFFICULTY_FLOOR);
    }

    #[test]
    fn empty_chain_returns_floor() {
        assert_eq!(calculate_next_difficulty(&[], 0), DIFFICULTY_FLOOR);
    }

    #[test]
    fn faster_blocks_increase_difficulty() {
        // 21 blocks at 1s intervals (much faster than TARGET_BLOCK_TIME = 30s).
        // now_secs = just after the last block (no stall triggered).
        let start = 1_700_000_000u64;
        let fast = make_chain(21, 1, 1_000, start);
        let slow = make_chain(21, TARGET_BLOCK_TIME, 1_000, start);
        let now_fast = fast.last().unwrap().header.timestamp + 1;
        let now_slow = slow.last().unwrap().header.timestamp + 1;

        let diff_fast = calculate_next_difficulty(&fast, now_fast);
        let diff_slow = calculate_next_difficulty(&slow, now_slow);
        assert!(
            diff_fast > diff_slow,
            "faster blocks must produce HIGHER difficulty (harder to mine), \
             got fast={diff_fast} slow={diff_slow}"
        );
    }

    #[test]
    fn slower_blocks_decrease_difficulty() {
        // 21 blocks at 120s intervals (much slower than TARGET_BLOCK_TIME = 30s).
        let start = 1_700_000_000u64;
        let slow = make_chain(21, 120, 1_000, start);
        let ideal = make_chain(21, TARGET_BLOCK_TIME, 1_000, start);
        let now_slow = slow.last().unwrap().header.timestamp + 1;
        let now_ideal = ideal.last().unwrap().header.timestamp + 1;

        let diff_slow = calculate_next_difficulty(&slow, now_slow);
        let diff_ideal = calculate_next_difficulty(&ideal, now_ideal);
        assert!(
            diff_slow < diff_ideal,
            "slower blocks must produce LOWER difficulty (easier to mine), \
             got slow={diff_slow} ideal={diff_ideal}"
        );
    }

    #[test]
    fn steady_state_difficulty_stays_stable() {
        // 21 blocks at exactly TARGET_BLOCK_TIME — difficulty should not change.
        let chain = make_chain(21, TARGET_BLOCK_TIME, 1_000, 0);
        let last_ts = chain.last().unwrap().header.timestamp;
        // now_secs = last_ts + TARGET_BLOCK_TIME (one normal block time in the future)
        let new_diff = calculate_next_difficulty(&chain, last_ts + TARGET_BLOCK_TIME);
        // Allow ±1% tolerance for integer rounding.
        let lo = 990u64;
        let hi = 1_010u64;
        assert!(
            new_diff >= lo && new_diff <= hi,
            "steady-state at diff=1000: expected [{lo},{hi}], got {new_diff}"
        );
    }

    #[test]
    fn stall_triggers_downshift() {
        // Chain with a stall: last timestamp far in the past.
        let chain = make_chain(21, TARGET_BLOCK_TIME, 1_000, 0);
        let last_ts = chain.last().unwrap().header.timestamp;
        // Trigger stall: now far beyond stall_threshold.
        let stall_now = last_ts + TARGET_BLOCK_TIME * STALL_MULTIPLIER * 2;
        let without_stall = last_ts + TARGET_BLOCK_TIME;

        let d_stall = calculate_next_difficulty(&chain, stall_now);
        let d_normal = calculate_next_difficulty(&chain, without_stall);
        assert!(
            d_stall < d_normal,
            "stall must downshift difficulty: stall={d_stall} normal={d_normal}"
        );
    }

    #[test]
    fn interval_clamping_prevents_manipulation() {
        // Two chains with the same average but one has extreme outlier intervals.
        // Without clamping, the manipulated chain would produce a very different difficulty.
        let normal = make_chain(21, TARGET_BLOCK_TIME, 1_000, 0);
        let last_ts_n = normal.last().unwrap().header.timestamp;

        // Chain where every other interval is 1s / 59s (same average = 30s but extremes).
        let mut manipulated: Vec<Block> = Vec::new();
        let mut ts = 0u64;
        for i in 0..21usize {
            let interval = if i % 2 == 0 {
                1
            } else {
                TARGET_BLOCK_TIME * 2 - 1
            };
            manipulated.push(make_block(i as u64, ts, 1_000));
            ts += interval;
        }
        let last_ts_m = manipulated.last().unwrap().header.timestamp;

        let d_normal = calculate_next_difficulty(&normal, last_ts_n + TARGET_BLOCK_TIME);
        let d_manip = calculate_next_difficulty(&manipulated, last_ts_m + TARGET_BLOCK_TIME);

        // With clamping, the manipulated chain's difficulty must not deviate wildly.
        // Allow up to 3× difference (without clamping it could be orders of magnitude).
        assert!(
            d_manip >= d_normal / 3 && d_manip <= d_normal * 3,
            "clamping must limit manipulation impact: normal={d_normal} manipulated={d_manip}"
        );
    }

    #[test]
    fn difficulty_floor_is_always_respected() {
        // Even with very slow blocks, difficulty must not go below DIFFICULTY_FLOOR.
        let chain = make_chain(21, 100_000, 1, 0);
        let d = calculate_next_difficulty(&chain, 1_000_000_000);
        assert_eq!(d, DIFFICULTY_FLOOR);
    }
}

use crate::config::constants::*;
use crate::pow::U256;
use once_cell::sync::Lazy;

// ─── Algorithm parameters ─────────────────────────────────────────────────────

/// VisionX algorithm parameters.
///
/// All fields are consensus-critical. Every miner and every validator must use
/// identical values. Parameters are defined in `config/constants.rs` [CONSENSUS];
/// they are gathered here into a single struct for handshake verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisionXParams {
    /// Base dataset size in megabytes.
    pub dataset_mb: usize,
    /// Per-hash scratchpad size in megabytes.
    pub scratch_mb: usize,
    /// Number of mix iterations per hash.
    pub mix_iters: u32,
    /// Dependent memory reads per mix iteration.
    pub reads_per_iter: u32,
    /// Dataset write-back stride (every N iterations).
    pub write_every: u32,
    /// Blocks per epoch (dataset is rebuilt once per epoch).
    pub epoch_blocks: u32,
}

impl Default for VisionXParams {
    fn default() -> Self {
        Self {
            dataset_mb:    VISIONX_DATASET_MB,
            scratch_mb:    VISIONX_SCRATCH_MB,
            mix_iters:     VISIONX_MIX_ITERS,
            reads_per_iter: VISIONX_READS_PER_ITER,
            write_every:   VISIONX_WRITE_EVERY,
            epoch_blocks:  VISIONX_EPOCH_BLOCKS,
        }
    }
}

impl VisionXParams {
    /// Stable blake3 fingerprint of the parameter set used during peer handshake.
    ///
    /// Two nodes with different VisionX parameters will produce different
    /// fingerprints and must be rejected at handshake.
    pub fn fingerprint(&self) -> String {
        let canonical = format!(
            "visionx/v1 dataset_mb={} scratch_mb={} mix_iters={} \
             reads_per_iter={} write_every={} epoch_blocks={}",
            self.dataset_mb, self.scratch_mb, self.mix_iters,
            self.reads_per_iter, self.write_every, self.epoch_blocks
        );
        hex::encode(blake3::hash(canonical.as_bytes()).as_bytes())
    }

    /// Return the epoch number for a given block height.
    ///
    /// Epoch changes trigger a full dataset rebuild at the miner.
    /// `epoch_blocks` is consensus-critical; do not read from config files.
    pub fn epoch(&self, height: u64) -> u64 {
        height / self.epoch_blocks as u64
    }
}

/// Global singleton of the canonical production VisionX params.
///
/// All code that needs algorithm parameters should reference this rather than
/// constructing a local `VisionXParams`. This guarantees a single definition.
pub static VISIONX_PARAMS: Lazy<VisionXParams> = Lazy::new(VisionXParams::default);

// --- Internal helpers ----------------------------------------------------

#[allow(dead_code)]
#[derive(Clone)]
struct SplitMix64 {
    state: u64,
}

#[allow(dead_code)]
impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline]
    fn next(&mut self) -> u64 {
        let mut z = {
            self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
            self.state
        };
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

#[allow(dead_code)]
#[inline]
fn expand_256(mut a: u64, mut b: u64) -> U256 {
    for _ in 0..4 {
        a = a.rotate_left(13) ^ b.wrapping_mul(0x9E3779B185EBCA87);
        b = b.rotate_left(17) ^ a.wrapping_mul(0xC2B2AE3D27D4EB4F);
    }
    let mut sm = SplitMix64::new(a ^ b ^ 0xD6E8FEB86659FD93);
    let c = sm.next();
    let d = sm.next();
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&a.to_be_bytes());
    out[8..16].copy_from_slice(&b.to_be_bytes());
    out[16..24].copy_from_slice(&c.to_be_bytes());
    out[24..32].copy_from_slice(&d.to_be_bytes());
    out
}

#[allow(dead_code)]
#[inline]
fn fold_seed(prev_hash32: &[u8; 32], epoch_id: u64) -> u64 {
    let mut s: u64 = epoch_id ^ 0xA24BAED4963EE407;
    for chunk in prev_hash32.chunks(8) {
        let mut v = [0u8; 8];
        v[..chunk.len()].copy_from_slice(chunk);
        s ^= u64::from_be_bytes(v).rotate_left(7);
        s = s.wrapping_mul(0x9E3779B97F4A7C15).rotate_left(9);
    }
    s
}

#[allow(dead_code)]
fn init_scratch(
    params: &VisionXParams,
    base: &[u64],
    base_mask: usize,
    header: &[u8],
    nonce: u64,
) -> (Vec<u64>, usize) {
    let bytes = params.scratch_mb * 1024 * 1024;
    let mut words = bytes / std::mem::size_of::<u64>();
    let mut n = 1usize;
    while n < words {
        n <<= 1;
    }
    words = n;
    let smask = words - 1;

    let mut seed: u64 = nonce ^ 0xDEADBEEFF00DFACE;
    for chunk in header.chunks(8) {
        let mut v = [0u8; 8];
        v[..chunk.len()].copy_from_slice(chunk);
        seed ^= u64::from_be_bytes(v).rotate_left(13);
        seed = seed.wrapping_mul(0x9E3779B97F4A7C15).rotate_left(7);
    }

    let mut scratch = vec![0u64; words];
    let mut sm = SplitMix64::new(seed);

    for i in 0..words {
        let mix_seed = sm.next();
        let idx1 = (mix_seed.rotate_left(17) as usize) & base_mask;
        let idx2 = (mix_seed.rotate_right(23) as usize) & base_mask;
        scratch[i] = base[idx1] ^ base[idx2] ^ mix_seed.wrapping_mul(0xC2B2AE3D27D4EB4F);
    }

    (scratch, smask)
}

// ─── Hash function (stub) ─────────────────────────────────────────────────────

/// Compute a VisionX PoW hash for the given header bytes and nonce.
///
/// **This is a stub implementation.** The body uses blake3 as a placeholder
/// so the rest of the codebase can compile and test end-to-end logic. Replace
/// this body with the full VisionX DAG algorithm before mainnet deployment.
///
/// The function signature and the meaning of its arguments are consensus-stable:
/// do not change them when replacing the body.
///
/// # Arguments
/// * `header_bytes` — canonical block header bytes (`BlockHeader::canonical_bytes`)
/// * `nonce`        — nonce being tested by the miner
/// * `_params`      — VisionX algorithm parameters (used by the real implementation)
///
/// # Returns
/// 32-byte hash value. A block is valid when this value ≤ `difficulty_to_target(difficulty)`.
pub fn compute_visionx_hash(
    header_bytes: &[u8],
    nonce: u64,
    _params: &VisionXParams,
) -> [u8; 32] {
    // STUB: blake3(header_bytes ++ nonce_le) — replace with VisionX DAG algorithm.
    let mut input = Vec::with_capacity(header_bytes.len() + 8);
    input.extend_from_slice(header_bytes);
    input.extend_from_slice(&nonce.to_le_bytes());
    *blake3::hash(&input).as_bytes()
}

/// Return the hex-encoded VisionX hash for a given header and nonce.
///
/// Convenience wrapper around `compute_visionx_hash` using the canonical
/// production parameters. Used by block producers and verifiers.
pub fn visionx_hash_hex(header_bytes: &[u8], nonce: u64) -> String {
    hex::encode(compute_visionx_hash(header_bytes, nonce, &VISIONX_PARAMS))
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── VisionXParams ────────────────────────────────────────────────────────

    #[test]
    fn params_default_matches_constants() {
        let p = VisionXParams::default();
        assert_eq!(p.dataset_mb,    VISIONX_DATASET_MB);
        assert_eq!(p.scratch_mb,    VISIONX_SCRATCH_MB);
        assert_eq!(p.mix_iters,     VISIONX_MIX_ITERS);
        assert_eq!(p.reads_per_iter, VISIONX_READS_PER_ITER);
        assert_eq!(p.write_every,   VISIONX_WRITE_EVERY);
        assert_eq!(p.epoch_blocks,  VISIONX_EPOCH_BLOCKS);
    }

    #[test]
    fn params_singleton_matches_default() {
        assert_eq!(*VISIONX_PARAMS, VisionXParams::default());
    }

    // ── fingerprint ──────────────────────────────────────────────────────────

    #[test]
    fn fingerprint_is_deterministic() {
        let p = VisionXParams::default();
        assert_eq!(p.fingerprint(), p.fingerprint());
    }

    #[test]
    fn fingerprint_is_hex_64_chars() {
        let f = VisionXParams::default().fingerprint();
        assert_eq!(f.len(), 64);
        assert!(f.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn fingerprint_changes_with_params() {
        let p1 = VisionXParams::default();
        let mut p2 = p1;
        p2.mix_iters += 1;
        assert_ne!(p1.fingerprint(), p2.fingerprint(),
            "changing any param must change the fingerprint");
    }

    // ── epoch ────────────────────────────────────────────────────────────────

    #[test]
    fn epoch_zero_for_genesis() {
        assert_eq!(VISIONX_PARAMS.epoch(0), 0);
    }

    #[test]
    fn epoch_increments_at_boundary() {
        let ep = VISIONX_EPOCH_BLOCKS as u64;
        assert_eq!(VISIONX_PARAMS.epoch(ep - 1), 0);
        assert_eq!(VISIONX_PARAMS.epoch(ep),     1);
        assert_eq!(VISIONX_PARAMS.epoch(ep * 2), 2);
    }

    // ── compute_visionx_hash ─────────────────────────────────────────────────

    #[test]
    fn compute_hash_is_deterministic() {
        let header = b"test_header_bytes";
        let nonce = 12345u64;
        let h1 = compute_visionx_hash(header, nonce, &VISIONX_PARAMS);
        let h2 = compute_visionx_hash(header, nonce, &VISIONX_PARAMS);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_nonces_give_different_hashes() {
        let header = b"test_header_bytes";
        let h1 = compute_visionx_hash(header, 0, &VISIONX_PARAMS);
        let h2 = compute_visionx_hash(header, 1, &VISIONX_PARAMS);
        assert_ne!(h1, h2, "different nonces must produce different hashes");
    }

    #[test]
    fn different_headers_give_different_hashes() {
        let h1 = compute_visionx_hash(b"header_a", 0, &VISIONX_PARAMS);
        let h2 = compute_visionx_hash(b"header_b", 0, &VISIONX_PARAMS);
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_output_is_32_bytes() {
        let result = compute_visionx_hash(b"header", 0, &VISIONX_PARAMS);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn visionx_hash_hex_is_deterministic() {
        let header = b"deterministic_header";
        assert_eq!(visionx_hash_hex(header, 42), visionx_hash_hex(header, 42));
    }

    #[test]
    fn visionx_hash_hex_is_64_chars() {
        let h = visionx_hash_hex(b"some_header", 0);
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
    #[test]
    fn splitmix64_sequence_is_deterministic() {
        let mut a = SplitMix64::new(0x1234_5678_9ABC_DEF0);
        let mut b = SplitMix64::new(0x1234_5678_9ABC_DEF0);

        let a1 = a.next();
        let b1 = b.next();
        let a2 = a.next();
        let b2 = b.next();

        assert_eq!(a1, b1);
        assert_eq!(a2, b2);
        assert_ne!(a1, a2);
    }

    #[test]
    fn fold_seed_is_deterministic_and_sensitive() {
        let prev = [0x11u8; 32];
        let same_a = fold_seed(&prev, 7);
        let same_b = fold_seed(&prev, 7);
        let diff_epoch = fold_seed(&prev, 8);
        let diff_prev = fold_seed(&[0x22u8; 32], 7);

        assert_eq!(same_a, same_b);
        assert_ne!(same_a, diff_epoch);
        assert_ne!(same_a, diff_prev);
    }

    #[test]
    fn expand_256_is_deterministic_and_32_bytes() {
        let a = expand_256(0x0123_4567_89AB_CDEF, 0x0FED_CBA9_8765_4321);
        let b = expand_256(0x0123_4567_89AB_CDEF, 0x0FED_CBA9_8765_4321);
        let c = expand_256(0x0123_4567_89AB_CDEF, 0x0FED_CBA9_8765_4322);

        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
        assert_ne!(a, c);
    }

    #[test]
    fn init_scratch_is_deterministic_with_small_params() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 1,
            reads_per_iter: 2,
            write_every: 1,
            epoch_blocks: 32,
        };
        let base = vec![
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
            0x9999_AAAA_BBBB_CCCC,
            0xDDDD_EEEE_FFFF_0000,
        ];
        let header = b"small-test-header";

        let (scratch_a, mask_a) = init_scratch(&params, &base, base.len() - 1, header, 42);
        let (scratch_b, mask_b) = init_scratch(&params, &base, base.len() - 1, header, 42);
        let (scratch_c, mask_c) = init_scratch(&params, &base, base.len() - 1, header, 43);

        assert_eq!(scratch_a, scratch_b);
        assert_eq!(mask_a, mask_b);
        assert_ne!(scratch_a, scratch_c);
        assert_eq!(mask_a & (mask_a + 1), 0);
        assert_eq!(mask_a, mask_c);
        assert_eq!(scratch_a.len() & mask_a, 0);
    }
}


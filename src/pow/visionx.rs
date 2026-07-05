use crate::config::constants::*;
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
}

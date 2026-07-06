use crate::config::constants::*;
use crate::pow::historical_vpow::historical_vpow_message_bytes_with_nonce_zero;
use crate::pow::U256;
use crate::types::BlockHeader;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// â”€â”€â”€ Algorithm parameters â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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
            dataset_mb: VISIONX_DATASET_MB,
            scratch_mb: VISIONX_SCRATCH_MB,
            mix_iters: VISIONX_MIX_ITERS,
            reads_per_iter: VISIONX_READS_PER_ITER,
            write_every: VISIONX_WRITE_EVERY,
            epoch_blocks: VISIONX_EPOCH_BLOCKS,
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
            self.dataset_mb,
            self.scratch_mb,
            self.mix_iters,
            self.reads_per_iter,
            self.write_every,
            self.epoch_blocks
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

#[allow(dead_code)]
type DatasetCache = HashMap<(u64, [u8; 32]), (Arc<Vec<u64>>, usize)>;

#[allow(dead_code)]
static DATASET_CACHE: Lazy<Mutex<DatasetCache>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[allow(dead_code)]
struct VisionXDataset {
    mem: Box<[u64]>,
    mask: usize,
}

#[allow(dead_code)]
impl VisionXDataset {
    fn build(params: &VisionXParams, prev_hash32: &[u8; 32], epoch: u64) -> Self {
        let bytes = params.dataset_mb * 1024 * 1024;
        let mut words = bytes / std::mem::size_of::<u64>();
        let mut n = 1usize;
        while n < words {
            n <<= 1;
        }
        words = n;

        let seed = fold_seed(prev_hash32, epoch);
        let mut sm = SplitMix64::new(seed);
        let mut mem = vec![0u64; words].into_boxed_slice();
        for i in 0..words {
            mem[i] = sm.next();
        }

        Self {
            mem,
            mask: words - 1,
        }
    }

    fn get_cached(
        params: &VisionXParams,
        prev_hash32: &[u8; 32],
        epoch: u64,
    ) -> (Arc<Vec<u64>>, usize) {
        let key = (epoch, *prev_hash32);

        {
            let cache = DATASET_CACHE.lock().unwrap();
            if let Some((dataset, mask)) = cache.get(&key) {
                return (Arc::clone(dataset), *mask);
            }
        }

        let ds = Self::build(params, prev_hash32, epoch);
        let dataset_arc = Arc::new(ds.mem.to_vec());
        let mask = ds.mask;

        {
            let mut cache = DATASET_CACHE.lock().unwrap();
            cache.insert(key, (Arc::clone(&dataset_arc), mask));
            if cache.len() > 3 {
                if let Some(oldest_key) = cache.keys().next().copied() {
                    cache.remove(&oldest_key);
                }
            }
        }

        (dataset_arc, mask)
    }

    fn clear_cache() {
        DATASET_CACHE.lock().unwrap().clear();
    }
}
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

#[allow(dead_code)]
fn visionx_hash(
    params: &VisionXParams,
    base: &[u64],
    base_mask: usize,
    header: &[u8],
    nonce: u64,
) -> U256 {
    let (mut scratch, smask) = init_scratch(params, base, base_mask, header, nonce);

    let mut a: u64 = 0x243F_6A88_85A3_08D3 ^ nonce.rotate_left(17);
    let mut b: u64 = 0x1319_8A2E_0370_7344 ^ nonce.rotate_right(11);

    for chunk in header.chunks(16) {
        let mut p = [0u8; 16];
        p[..chunk.len()].copy_from_slice(chunk);
        let mut x_bytes = [0u8; 8];
        let mut y_bytes = [0u8; 8];
        x_bytes.copy_from_slice(&p[0..8]);
        y_bytes.copy_from_slice(&p[8..16]);
        let x = u64::from_be_bytes(x_bytes);
        let y = u64::from_be_bytes(y_bytes);
        a ^= x.wrapping_mul(0x9E37_79B1_85EB_CA87);
        b ^= y.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        a = a.rotate_left(13) ^ b.rotate_right(7);
        b = b.rotate_left(29) ^ a.rotate_right(19);
    }

    let its = params.mix_iters;
    let mut acc = a ^ b ^ 0xDEAD_BEEF_F00D_FACEu64;
    let writes = params.write_every;

    for i in 0..its {
        let j1 =
            (a ^ b ^ acc ^ (i as u64).wrapping_mul(0x9E3779B9)).rotate_left(17) as usize & smask;
        let v1 = scratch[j1];

        let j2 = (v1 ^ a ^ acc).rotate_left(23) as usize & smask;
        let v2 = scratch[j2];

        let j3 = (v2 ^ b ^ acc).rotate_left(19) as usize & smask;
        let v3 = scratch[j3];

        let v4 = if params.reads_per_iter >= 4 {
            let j4 = (v3 ^ v1 ^ acc).rotate_left(29) as usize & smask;
            scratch[j4]
        } else {
            v3
        };

        let mix =
            v1 ^ v2.rotate_left(13) ^ v3.wrapping_mul(0x94D0_49BB_1331_11EB) ^ v4.rotate_right(7);

        a = a.rotate_left(13) ^ mix.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        b = b.rotate_left(17) ^ (mix ^ acc).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        acc = acc.rotate_left(7) ^ (a ^ b).wrapping_mul(0xD6E8_FEB8_6659_FD93);

        if writes > 0 && (i % writes) == 0 {
            let jw = (mix ^ a ^ b.rotate_left(11) ^ (i as u64).wrapping_mul(0xA24B_AED4_963E_E407))
                .rotate_left(31) as usize
                & smask;
            scratch[jw] = scratch[jw]
                .wrapping_add(mix ^ 0x9E37_79B9_7F4A_7C15)
                .rotate_left(41);
        }
    }

    expand_256(a ^ acc, b ^ acc.rotate_left(3))
}

fn nonce_from_header(header_with_nonce: &[u8], nonce_offset: usize) -> Option<u64> {
    let end = nonce_offset.checked_add(8)?;
    let nonce_bytes = header_with_nonce.get(nonce_offset..end)?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(nonce_bytes);
    Some(u64::from_be_bytes(raw))
}

fn meets_target(hash: &U256, target: &U256) -> bool {
    &hash[0..8] <= &target[0..8]
}

fn params_within_bounds(params: &VisionXParams) -> bool {
    params.dataset_mb <= 512
        && params.scratch_mb <= 128
        && params.mix_iters <= 1_000_000
        && params.reads_per_iter <= 8
}

fn decode_hash_32(s: &str) -> Result<[u8; 32], String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|e| format!("parent_hash: invalid hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "parent_hash: expected 32 bytes, got {}",
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub(crate) fn visionx_digest(
    params: &VisionXParams,
    prev_hash32: &[u8; 32],
    epoch: u64,
    header_bytes: &[u8],
    nonce: u64,
) -> Result<U256, String> {
    if !params_within_bounds(params) {
        return Err("invalid VisionX parameters".into());
    }

    let (dataset, mask) = VisionXDataset::get_cached(params, prev_hash32, epoch);
    Ok(visionx_hash(
        params,
        dataset.as_slice(),
        mask,
        header_bytes,
        nonce,
    ))
}

/// Compute the historical VisionX digest for a block header.
///
/// The header nonce is not embedded into the historical preimage; it is passed
/// separately to the VisionX hashing engine.
pub(crate) fn historical_block_digest(
    params: &VisionXParams,
    epoch: u64,
    header: &BlockHeader,
) -> Result<U256, String> {
    let prev_hash32 = decode_hash_32(&header.parent_hash)?;
    let historical_preimage = historical_vpow_message_bytes_with_nonce_zero(header)?;
    visionx_digest(
        params,
        &prev_hash32,
        epoch,
        historical_preimage.as_slice(),
        header.nonce,
    )
}

/// Verify a historical VisionX candidate.
///
/// Returns `false` on malformed nonce offsets, undersized buffers, or hashes
/// that do not satisfy the target. The comparison follows the historical
/// upper-64-bit rule only.
pub fn verify(
    params: &VisionXParams,
    prev_hash32: &[u8; 32],
    epoch: u64,
    header_with_nonce: &[u8],
    nonce_offset: usize,
    target: &U256,
) -> bool {
    if !params_within_bounds(params) {
        return false;
    }

    let Some(nonce) = nonce_from_header(header_with_nonce, nonce_offset) else {
        return false;
    };

    let (dataset, mask) = VisionXDataset::get_cached(params, prev_hash32, epoch);
    let digest = visionx_hash(params, dataset.as_slice(), mask, header_with_nonce, nonce);
    meets_target(&digest, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    // â”€â”€ VisionXParams â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn params_default_matches_constants() {
        let p = VisionXParams::default();
        assert_eq!(p.dataset_mb, VISIONX_DATASET_MB);
        assert_eq!(p.scratch_mb, VISIONX_SCRATCH_MB);
        assert_eq!(p.mix_iters, VISIONX_MIX_ITERS);
        assert_eq!(p.reads_per_iter, VISIONX_READS_PER_ITER);
        assert_eq!(p.write_every, VISIONX_WRITE_EVERY);
        assert_eq!(p.epoch_blocks, VISIONX_EPOCH_BLOCKS);
    }

    #[test]
    fn params_singleton_matches_default() {
        assert_eq!(*VISIONX_PARAMS, VisionXParams::default());
    }

    // â”€â”€ fingerprint â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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
        assert_ne!(
            p1.fingerprint(),
            p2.fingerprint(),
            "changing any param must change the fingerprint"
        );
    }

    // â”€â”€ epoch â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn epoch_zero_for_genesis() {
        assert_eq!(VISIONX_PARAMS.epoch(0), 0);
    }

    #[test]
    fn epoch_increments_at_boundary() {
        let ep = VISIONX_EPOCH_BLOCKS as u64;
        assert_eq!(VISIONX_PARAMS.epoch(ep - 1), 0);
        assert_eq!(VISIONX_PARAMS.epoch(ep), 1);
        assert_eq!(VISIONX_PARAMS.epoch(ep * 2), 2);
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
    #[test]
    fn dataset_build_small_is_deterministic() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 1,
            reads_per_iter: 2,
            write_every: 1,
            epoch_blocks: 32,
        };
        let prev = [0x11u8; 32];
        let a = VisionXDataset::build(&params, &prev, 7);
        let b = VisionXDataset::build(&params, &prev, 7);

        assert_eq!(a.mem, b.mem);
        assert_eq!(a.mask, b.mask);
        assert!(a.mem.len() > 0);
        assert_eq!(a.mem.len() & a.mask, 0);
    }

    #[test]
    fn dataset_build_small_changes_with_epoch_and_prev_hash() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 1,
            reads_per_iter: 2,
            write_every: 1,
            epoch_blocks: 32,
        };
        let prev_a = [0x11u8; 32];
        let prev_b = [0x22u8; 32];
        let a = VisionXDataset::build(&params, &prev_a, 7);
        let b = VisionXDataset::build(&params, &prev_a, 8);
        let c = VisionXDataset::build(&params, &prev_b, 7);

        assert_ne!(a.mem, b.mem);
        assert_ne!(a.mem, c.mem);
        assert_eq!(a.mask, b.mask);
        assert_eq!(a.mask, c.mask);
    }

    #[test]
    fn dataset_cache_reuses_small_allocation_for_same_key() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 1,
            reads_per_iter: 2,
            write_every: 1,
            epoch_blocks: 32,
        };
        let prev = [0x33u8; 32];
        VisionXDataset::clear_cache();
        let (a, mask_a) = VisionXDataset::get_cached(&params, &prev, 3);
        let (b, mask_b) = VisionXDataset::get_cached(&params, &prev, 3);

        assert_eq!(mask_a, mask_b);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn dataset_cache_distinguishes_epoch_and_prev_hash() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 1,
            reads_per_iter: 2,
            write_every: 1,
            epoch_blocks: 32,
        };
        let prev = [0x44u8; 32];
        VisionXDataset::clear_cache();
        let (a, _) = VisionXDataset::get_cached(&params, &prev, 3);
        let (b, _) = VisionXDataset::get_cached(&params, &prev, 4);
        let (c, _) = VisionXDataset::get_cached(&params, &[0x55u8; 32], 3);

        assert!(!Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&a, &c));
    }
    #[test]
    fn visionx_hash_small_params_is_deterministic() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let dataset = VisionXDataset::build(&params, &prev, 9);
        let header = b"visionx-small-test";
        let h1 = visionx_hash(
            &params,
            &dataset.mem,
            dataset.mask,
            header,
            0xA5A5_A5A5_A5A5_A5A5,
        );
        let h2 = visionx_hash(
            &params,
            &dataset.mem,
            dataset.mask,
            header,
            0xA5A5_A5A5_A5A5_A5A5,
        );

        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
    }

    #[test]
    fn visionx_hash_changes_with_nonce() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let dataset = VisionXDataset::build(&params, &prev, 9);
        let header = b"visionx-small-test";
        let h1 = visionx_hash(&params, &dataset.mem, dataset.mask, header, 1);
        let h2 = visionx_hash(&params, &dataset.mem, dataset.mask, header, 2);

        assert_ne!(h1, h2);
    }

    #[test]
    fn visionx_hash_changes_with_header() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let dataset = VisionXDataset::build(&params, &prev, 9);
        let h1 = visionx_hash(&params, &dataset.mem, dataset.mask, b"header-a", 1);
        let h2 = visionx_hash(&params, &dataset.mem, dataset.mask, b"header-b", 1);

        assert_ne!(h1, h2);
    }

    #[test]
    fn visionx_verify_accepts_valid_candidate() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let epoch = 9;
        let nonce_offset = 8usize;
        let mut header = vec![0x11u8; 24];
        header[nonce_offset..nonce_offset + 8]
            .copy_from_slice(&0xA5A5_A5A5_A5A5_A5A5u64.to_be_bytes());
        let target = [0xFFu8; 32];

        assert!(verify(
            &params,
            &prev,
            epoch,
            &header,
            nonce_offset,
            &target
        ));
    }

    #[test]
    fn visionx_verify_rejects_invalid_nonce_offset() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let epoch = 9;
        let header = vec![0x11u8; 16];
        let target = [0xFFu8; 32];

        assert!(!verify(&params, &prev, epoch, &header, 20, &target));
    }

    #[test]
    fn visionx_verify_rejects_undersized_buffers() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let epoch = 9;
        let header = vec![0x11u8; 7];
        let target = [0xFFu8; 32];

        assert!(!verify(&params, &prev, epoch, &header, 0, &target));
    }

    #[test]
    fn visionx_verify_ignores_lower_target_bytes() {
        let params = VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        };
        let prev = [0x66u8; 32];
        let epoch = 9;
        let nonce_offset = 8usize;
        let mut header = vec![0x11u8; 24];
        header[nonce_offset..nonce_offset + 8]
            .copy_from_slice(&0xA5A5_A5A5_A5A5_A5A5u64.to_be_bytes());
        let mut target = [0xFFu8; 32];
        target[8..].fill(0x00);

        assert!(verify(
            &params,
            &prev,
            epoch,
            &header,
            nonce_offset,
            &target
        ));
    }
}

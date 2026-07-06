use crate::pow::difficulty::U256;
use crate::pow::historical_vpow::historical_vpow_message_bytes_with_nonce_zero;
use crate::pow::visionx::{compute_visionx_hash, verify, VisionXParams};
use crate::types::BlockHeader;

fn inject_nonce(header_bytes: &[u8], nonce_offset: usize, nonce: u64) -> Result<Vec<u8>, String> {
    let end = nonce_offset
        .checked_add(8)
        .ok_or_else(|| format!("nonce offset {} overflows", nonce_offset))?;
    let mut out = header_bytes.to_vec();
    if out.len() < end {
        return Err(format!(
            "nonce offset {} needs {} bytes, got {}",
            nonce_offset,
            end,
            out.len()
        ));
    }
    out[nonce_offset..end].copy_from_slice(&nonce.to_be_bytes());
    Ok(out)
}

/// Mining job for historical VisionX PoW.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowJob {
    pub params: VisionXParams,
    pub prev_hash32: [u8; 32],
    pub epoch: u64,
    pub header_bytes: Vec<u8>,
    pub nonce_offset: usize,
    pub target: U256,
    historical_preimage: Option<Vec<u8>>,
}

impl PowJob {
    pub fn new(
        params: VisionXParams,
        prev_hash32: [u8; 32],
        epoch: u64,
        header_bytes: Vec<u8>,
        nonce_offset: usize,
        target: U256,
    ) -> Result<Self, String> {
        let _ = inject_nonce(&header_bytes, nonce_offset, 0)?;
        Ok(Self {
            params,
            prev_hash32,
            epoch,
            header_bytes,
            nonce_offset,
            target,
            historical_preimage: None,
        })
    }

    fn from_historical_header(
        params: VisionXParams,
        prev_hash32: [u8; 32],
        epoch: u64,
        header: &BlockHeader,
        nonce_offset: usize,
        target: U256,
    ) -> Result<Self, String> {
        let historical_preimage = historical_vpow_message_bytes_with_nonce_zero(header)?;
        Ok(Self {
            params,
            prev_hash32,
            epoch,
            header_bytes: historical_preimage.clone(),
            nonce_offset,
            target,
            historical_preimage: Some(historical_preimage),
        })
    }

    pub fn header_with_nonce(&self, nonce: u64) -> Result<Vec<u8>, String> {
        inject_nonce(&self.header_bytes, self.nonce_offset, nonce)
    }

    fn mining_input_bytes(&self) -> &[u8] {
        self.historical_preimage
            .as_deref()
            .unwrap_or(self.header_bytes.as_slice())
    }

    pub fn solution_for_nonce(&self, nonce: u64) -> Result<PowSolution, String> {
        let hash = compute_visionx_hash(self.mining_input_bytes(), nonce, &self.params);
        Ok(PowSolution { nonce, hash })
    }

    pub fn validate_solution(&self, solution: &PowSolution) -> bool {
        let Ok(_header_with_nonce) = self.header_with_nonce(solution.nonce) else {
            return false;
        };

        let expected_hash = compute_visionx_hash(self.mining_input_bytes(), solution.nonce, &self.params);
        if solution.hash != expected_hash {
            return false;
        }

        if self.historical_preimage.is_some() {
            return &solution.hash[0..8] <= &self.target[0..8];
        }

        verify(
            &self.params,
            &self.prev_hash32,
            self.epoch,
            &self.header_bytes,
            self.nonce_offset,
            &self.target,
        )
    }
}

/// Found VisionX solution candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowSolution {
    pub nonce: u64,
    pub hash: U256,
}

impl PowSolution {
    pub fn new(nonce: u64, hash: U256) -> Self {
        Self { nonce, hash }
    }
}

/// Simple VisionX miner that searches nonces for a given job.
#[derive(Debug, Clone)]
pub struct VisionXMiner {
    params: VisionXParams,
}

impl VisionXMiner {
    pub fn new(params: VisionXParams) -> Self {
        Self { params }
    }

    pub fn params(&self) -> VisionXParams {
        self.params
    }

    pub fn build_job(
        &self,
        prev_hash32: [u8; 32],
        epoch: u64,
        header_bytes: Vec<u8>,
        nonce_offset: usize,
        target: U256,
    ) -> Result<PowJob, String> {
        PowJob::new(self.params, prev_hash32, epoch, header_bytes, nonce_offset, target)
    }

    fn build_historical_job(
        &self,
        prev_hash32: [u8; 32],
        epoch: u64,
        header: &BlockHeader,
        nonce_offset: usize,
        target: U256,
    ) -> Result<PowJob, String> {
        PowJob::from_historical_header(self.params, prev_hash32, epoch, header, nonce_offset, target)
    }

    pub fn mine(&self, job: &PowJob, nonce_limit: u64) -> Option<PowSolution> {
        if job.params != self.params {
            return None;
        }
        for nonce in 0..nonce_limit {
            let candidate = match job.solution_for_nonce(nonce) {
                Ok(candidate) => candidate,
                Err(_) => return None,
            };
            if job.validate_solution(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    pub fn verify_solution(&self, job: &PowJob, solution: &PowSolution) -> bool {
        if job.params != self.params {
            return false;
        }
        job.validate_solution(solution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_params() -> VisionXParams {
        VisionXParams {
            dataset_mb: 1,
            scratch_mb: 1,
            mix_iters: 32,
            reads_per_iter: 4,
            write_every: 4,
            epoch_blocks: 32,
        }
    }

    fn small_job(target: U256) -> PowJob {
        let miner = VisionXMiner::new(small_params());
        miner
            .build_job([0x44u8; 32], 7, vec![0x11u8; 24], 8, target)
            .unwrap()
    }

    fn historical_sample_header(nonce: u64) -> BlockHeader {
        BlockHeader {
            parent_hash: "0x".to_string() + &"11".repeat(32),
            number: 12_345,
            timestamp: 1_700_000_000,
            difficulty: 1_000,
            nonce,
            pow_hash: String::new(),
            state_root: "0x".to_string() + &"22".repeat(32),
            tx_root: "0x".to_string() + &"33".repeat(32),
            miner: "pow_miner".to_string(),
        }
    }

    #[test]
    fn job_creation_preserves_inputs() {
        let params = small_params();
        let miner = VisionXMiner::new(params);
        let target = [0xFFu8; 32];
        let job = miner
            .build_job([0x22u8; 32], 9, vec![0x33u8; 24], 8, target)
            .unwrap();

        assert_eq!(job.params, params);
        assert_eq!(job.prev_hash32, [0x22u8; 32]);
        assert_eq!(job.epoch, 9);
        assert_eq!(job.header_bytes, vec![0x33u8; 24]);
        assert_eq!(job.nonce_offset, 8);
        assert_eq!(job.target, target);
    }

    #[test]
    fn solution_validation_accepts_matching_nonce() {
        let job = small_job([0xFFu8; 32]);
        let solution = job.solution_for_nonce(0).unwrap();

        assert!(job.validate_solution(&solution));
    }

    #[test]
    fn miner_finds_easy_target() {
        let miner = VisionXMiner::new(small_params());
        let job = miner
            .build_job([0x44u8; 32], 7, vec![0x11u8; 24], 8, [0xFFu8; 32])
            .unwrap();

        let solution = miner.mine(&job, 1).expect("nonce 0 should satisfy the easy target");
        assert_eq!(solution.nonce, 0);
        assert!(miner.verify_solution(&job, &solution));
    }

    #[test]
    fn miner_rejects_invalid_solution() {
        let miner = VisionXMiner::new(small_params());
        let job = miner
            .build_job([0x44u8; 32], 7, vec![0x11u8; 24], 8, [0x00u8; 32])
            .unwrap();
        let solution = PowSolution::new(0, compute_visionx_hash(&job.header_bytes, 0, &job.params));

        assert!(!miner.verify_solution(&job, &solution));
        assert!(miner.mine(&job, 1).is_none());
    }

    #[test]
    fn historical_preimage_matches_documented_vector() {
        let header = historical_sample_header(0);
        let bytes = historical_vpow_message_bytes_with_nonce_zero(&header)
            .expect("historical preimage should encode");

        assert_eq!(bytes.len(), 117);
        assert_eq!(hex::encode(bytes), "56504f57010000001111111111111111111111111111111111111111111111111111111111111111393000000000000000f1536500000000e8030000000000000000000000000000333333333333333333333333333333333333333333333333333333333333333309000000706f775f6d696e6572");
    }

    #[test]
    fn historical_mining_input_is_deterministic() {
        let miner = VisionXMiner::new(small_params());
        let header = historical_sample_header(42);
        let a = miner
            .build_historical_job([0x55u8; 32], 3, &header, 64, [0xFFu8; 32])
            .unwrap();
        let b = miner
            .build_historical_job([0x55u8; 32], 3, &header, 64, [0xFFu8; 32])
            .unwrap();

        assert_eq!(a.header_bytes, b.header_bytes);
        assert_eq!(a.mining_input_bytes(), b.mining_input_bytes());
        assert_eq!(a.mining_input_bytes(), historical_vpow_message_bytes_with_nonce_zero(&header).unwrap().as_slice());
    }

    #[test]
    fn nonce_changes_only_nonce_dependent_data() {
        let job = VisionXMiner::new(small_params())
            .build_historical_job([0x55u8; 32], 3, &historical_sample_header(0), 64, [0xFFu8; 32])
            .unwrap();
        let zero = job.header_with_nonce(0).unwrap();
        let one = job.header_with_nonce(1).unwrap();

        assert_eq!(zero.len(), one.len());
        assert_eq!(&zero[..64], &one[..64]);
        assert_ne!(&zero[64..72], &one[64..72]);
        assert_eq!(&zero[72..], &one[72..]);
    }

    #[test]
    fn historical_job_uses_compatibility_encoding() {
        let miner = VisionXMiner::new(small_params());
        let job = miner
            .build_historical_job([0x55u8; 32], 3, &historical_sample_header(0), 64, [0xFFu8; 32])
            .unwrap();
        let solution = job.solution_for_nonce(0).unwrap();

        assert!(job.validate_solution(&solution));
        assert!(miner.verify_solution(&job, &solution));
    }
}


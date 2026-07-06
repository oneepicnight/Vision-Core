use crate::pow::difficulty::U256;
use crate::pow::visionx::{compute_visionx_hash, verify, VisionXParams};

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
        })
    }

    pub fn header_with_nonce(&self, nonce: u64) -> Result<Vec<u8>, String> {
        inject_nonce(&self.header_bytes, self.nonce_offset, nonce)
    }

    pub fn solution_for_nonce(&self, nonce: u64) -> Result<PowSolution, String> {
        let hash = compute_visionx_hash(&self.header_bytes, nonce, &self.params);
        Ok(PowSolution { nonce, hash })
    }

    pub fn validate_solution(&self, solution: &PowSolution) -> bool {
        let Ok(header_with_nonce) = self.header_with_nonce(solution.nonce) else {
            return false;
        };
        let expected_hash = compute_visionx_hash(&self.header_bytes, solution.nonce, &self.params);
        if solution.hash != expected_hash {
            return false;
        }
        verify(
            &self.params,
            &self.prev_hash32,
            self.epoch,
            &header_with_nonce,
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
}


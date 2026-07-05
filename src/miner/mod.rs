pub mod job;
pub mod manager;

pub use manager::{MinerManager, MiningStats};
pub use job::{MiningJob, build_candidate, block_reward};

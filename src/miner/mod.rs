pub mod job;
pub mod manager;

pub use job::{block_reward, build_candidate, MiningJob};
pub use manager::{MinerManager, MiningStats};

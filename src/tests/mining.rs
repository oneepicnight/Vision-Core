#[cfg(test)]
mod tests {
    use crate::miner::manager::MinerManager;
    use crate::pow::visionx::VisionXParams;

    #[test]
    fn miner_manager_clear_job() {
        let mgr = MinerManager::new(VisionXParams::default());
        assert!(!mgr.is_mining());
        mgr.clear_job(); // must not panic when no job active
    }
}

use tokio::runtime::{Builder, Runtime};

/// Build the Tokio multi-thread runtime used for all async tasks.
///
/// Thread count defaults to the number of logical CPUs. Override with
/// `TOKIO_WORKER_THREADS` environment variable.
pub fn build_runtime() -> Runtime {
    let threads = std::env::var("TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(num_cpus);

    Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .thread_name("vision-worker")
        .build()
        .expect("Failed to build Tokio runtime")
}

fn num_cpus() -> usize {
    // std::thread::available_parallelism is stable since Rust 1.59.
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Output};

    use super::build_runtime;

    const RUNTIME_PROBE_SCENARIO: &str = "VISION_TEST_RUNTIME_PROBE_SCENARIO";

    fn run_runtime_probe(worker_threads: Option<&str>) -> Output {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .arg("--exact")
            .arg("node::runtime::tests::runtime_subprocess_probe")
            .arg("--test-threads=1")
            .env(RUNTIME_PROBE_SCENARIO, "build")
            .env_remove("TOKIO_WORKER_THREADS");

        if let Some(worker_threads) = worker_threads {
            command.env("TOKIO_WORKER_THREADS", worker_threads);
        }

        command.output().expect("runtime probe should start")
    }

    fn assert_probe_succeeds(worker_threads: Option<&str>) {
        let output = run_runtime_probe(worker_threads);
        assert!(
            output.status.success(),
            "runtime probe for {worker_threads:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn runtime_uses_logical_cpu_default_when_unset() {
        assert_probe_succeeds(None);
    }

    #[test]
    fn runtime_accepts_positive_worker_thread_override() {
        assert_probe_succeeds(Some("2"));
    }

    #[test]
    fn runtime_invalid_worker_thread_override_uses_current_fallback() {
        assert_probe_succeeds(Some("invalid"));
    }

    #[test]
    fn runtime_zero_worker_thread_override_hits_current_build_failure_boundary() {
        let output = run_runtime_probe(Some("0"));
        assert!(
            !output.status.success(),
            "zero worker-thread probe unexpectedly succeeded"
        );
        let output_text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output_text.contains("Worker threads cannot be set to 0"),
            "zero worker-thread failure did not identify the boundary\noutput:\n{output_text}"
        );
    }

    #[test]
    fn runtime_subprocess_probe() {
        let Ok(scenario) = std::env::var(RUNTIME_PROBE_SCENARIO) else {
            return;
        };
        assert_eq!(scenario, "build");
        drop(build_runtime());
    }
}

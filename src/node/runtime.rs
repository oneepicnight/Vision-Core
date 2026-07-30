use std::fmt;

use tokio::runtime::{Builder, Runtime};

#[derive(Debug)]
pub enum RuntimeError {
    InvalidWorkerThreads { value: String },
    Build(std::io::Error),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorkerThreads { value } => write!(
                formatter,
                "invalid TOKIO_WORKER_THREADS value {value:?}: expected a positive integer"
            ),
            Self::Build(error) => write!(formatter, "failed to build Tokio runtime: {error}"),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidWorkerThreads { .. } => None,
            Self::Build(error) => Some(error),
        }
    }
}

/// Build the Tokio multi-thread runtime used for all async tasks.
///
/// Thread count defaults to the number of logical CPUs. Override with
/// `TOKIO_WORKER_THREADS` environment variable.
pub fn build_runtime() -> Result<Runtime, RuntimeError> {
    let threads = parse_worker_threads(std::env::var("TOKIO_WORKER_THREADS").ok())?;

    Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .thread_name("vision-worker")
        .build()
        .map_err(RuntimeError::Build)
}

fn parse_worker_threads(raw: Option<String>) -> Result<usize, RuntimeError> {
    let Some(value) = raw else {
        return Ok(num_cpus());
    };

    match value.parse::<usize>() {
        Ok(threads) if threads > 0 => Ok(threads),
        _ => Err(RuntimeError::InvalidWorkerThreads { value }),
    }
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

    use super::{build_runtime, parse_worker_threads};

    const RUNTIME_PROBE_SCENARIO: &str = "VISION_TEST_RUNTIME_PROBE_SCENARIO";

    fn run_runtime_probe(worker_threads: Option<&str>) -> Output {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .arg("--exact")
            .arg("node::runtime::tests::runtime_subprocess_probe")
            .arg("--nocapture")
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
    fn runtime_rejects_malformed_worker_thread_override() {
        let output = run_runtime_probe(Some("invalid"));
        assert_probe_rejected(&output, "invalid");
    }

    #[test]
    fn runtime_rejects_zero_worker_thread_override() {
        let output = run_runtime_probe(Some("0"));
        assert_probe_rejected(&output, "0");
    }

    #[test]
    fn worker_thread_parser_rejects_negative_whitespace_and_overflow_values() {
        for value in [
            "-1",
            " 2 ",
            "18446744073709551616",
            "999999999999999999999999999999999999999999",
        ] {
            let error = parse_worker_threads(Some(value.to_string()))
                .expect_err("invalid worker-thread value should be rejected");
            assert_eq!(
                error.to_string(),
                format!(
                    "invalid TOKIO_WORKER_THREADS value {value:?}: expected a positive integer"
                )
            );
        }
    }

    #[test]
    fn worker_thread_parser_accepts_positive_bounds() {
        assert_eq!(parse_worker_threads(Some("1".to_string())).unwrap(), 1);
        assert_eq!(
            parse_worker_threads(Some(usize::MAX.to_string())).unwrap(),
            usize::MAX
        );
    }

    fn assert_probe_rejected(output: &Output, value: &str) {
        assert!(!output.status.success(), "probe unexpectedly succeeded");
        let output_text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output_text.contains(&format!(
                "invalid TOKIO_WORKER_THREADS value {value:?}: expected a positive integer"
            )),
            "worker-thread failure did not identify the invalid setting\noutput:\n{output_text}"
        );
    }

    #[test]
    fn runtime_subprocess_probe() {
        let Ok(scenario) = std::env::var(RUNTIME_PROBE_SCENARIO) else {
            return;
        };
        assert_eq!(scenario, "build");
        match build_runtime() {
            Ok(runtime) => drop(runtime),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }
}

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

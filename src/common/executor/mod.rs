use futures::Future;
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
};

static COMMON_THREAD_CORES: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| {
    std::env::var(crate::api::constants::ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
        .ok()
        .and_then(|v| v.parse::<usize>().ok().filter(|n| *n > 0))
        .unwrap_or(1)
});

static RT: std::sync::LazyLock<Runtime> = std::sync::LazyLock::new(|| {
    Builder::new_multi_thread()
        .enable_all()
        .thread_name("nacos-client-thread-pool")
        .worker_threads(*COMMON_THREAD_CORES)
        .build()
        .expect("Thread pool should build successfully with valid configuration")
});

pub(crate) fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RT.spawn(future)
}

pub(crate) fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    RT.spawn_blocking(f)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // Tests need std::thread::sleep and std::env::set_var
mod tests {

    use super::*;
    use crate::api::constants::ENV_NACOS_CLIENT_COMMON_THREAD_CORES;

    #[test]
    fn test_common_thread_cores() {
        let num_cpus = std::env::var(ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
            .ok()
            .and_then(|v| v.parse::<usize>().ok().filter(|n| *n > 0))
            .unwrap_or(
                std::thread::available_parallelism()
                    .expect("Should get available parallelism")
                    .get(),
            );
        assert!(num_cpus > 0);

        // SAFETY: This is safe in test context where we're the only thread
        // and setting an env var that only affects this test.
        // Rust 1.77+ marks set_var as unsafe due to thread-safety concerns
        // in multi-threaded contexts.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var(ENV_NACOS_CLIENT_COMMON_THREAD_CORES, "4");
        }
        let num_cpus = std::env::var(ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
            .ok()
            .and_then(|v| v.parse::<usize>().ok().filter(|n| *n > 0))
            .unwrap_or(
                std::thread::available_parallelism()
                    .expect("Should get available parallelism")
                    .get(),
            );
        assert_eq!(num_cpus, 4);
    }

    #[test]
    fn test_spawn() {
        let handler = spawn(async {
            println!("test spawn task");
            5
        });
        let ret = RT.block_on(handler);
        let ret = ret.expect("Runtime block_on should succeed");
        assert_eq!(ret, 5);
    }
}

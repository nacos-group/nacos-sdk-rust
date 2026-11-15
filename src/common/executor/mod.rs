use futures::Future;
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
};

fn parse_thread_cores(value: &str) -> Option<usize> {
    value.parse::<usize>().ok().filter(|n| *n > 0)
}

static COMMON_THREAD_CORES: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| {
    std::env::var(crate::api::constants::ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
        .ok()
        .and_then(|value| parse_thread_cores(&value))
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
mod tests {

    use super::*;

    #[test]
    fn test_parse_thread_cores() {
        assert_eq!(Some(4), parse_thread_cores("4"));
        assert_eq!(None, parse_thread_cores("0"));
        assert_eq!(None, parse_thread_cores("-1"));
        assert_eq!(None, parse_thread_cores("not-a-number"));
        assert!(*COMMON_THREAD_CORES > 0);
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

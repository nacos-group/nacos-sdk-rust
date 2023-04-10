use crate::api::error::Result;
use futures::Future;
use lazy_static::lazy_static;
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
    time::{interval, sleep, Duration},
};
use tracing::{error, Instrument};

lazy_static! {
    static ref COMMON_THREAD_CORES: usize = std::env::var(crate::api::constants::ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
        .ok()
        .and_then(|v| v.parse::<usize>().ok().filter(|n| *n > 0))
        .unwrap_or(std::thread::available_parallelism().unwrap().get()); // default is num_cpus

    static ref RT: Runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("nacos-client-thread-pool")
        .worker_threads(*COMMON_THREAD_CORES)
        .build()
        .unwrap();
}

pub(crate) fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RT.spawn(future)
}

#[allow(dead_code)]
pub(crate) fn schedule<F>(future: F, delay: Duration) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RT.spawn(async move {
        sleep(delay).await;
        future.await
    })
}

#[allow(dead_code)]
pub(crate) fn schedule_at_fixed_rate(
    task: impl Fn() -> Result<()> + Send + Sync + 'static,
    duration: Duration,
) -> JoinHandle<()> {
    RT.spawn(
        async move {
            loop {
                let ret = async { task() }.await;
                if let Err(e) = ret {
                    error!("schedule_at_fixed_rate occur an error: {e}");
                    break;
                }
                sleep(duration).await;
            }
        }
        .in_current_span(),
    )
}

#[allow(dead_code)]
pub(crate) fn schedule_at_fixed_delay(
    task: impl Fn() -> Result<()> + Send + Sync + 'static,
    duration: Duration,
) -> JoinHandle<()> {
    RT.spawn(
        async move {
            let mut interval = interval(duration);
            loop {
                interval.tick().await;
                let ret = async { task() }.await;
                if let Err(e) = ret {
                    error!("schedule_at_fixed_delay occur an error: {e}");
                    break;
                }
            }
        }
        .in_current_span(),
    )
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::api::constants::ENV_NACOS_CLIENT_COMMON_THREAD_CORES;

    #[test]
    fn test_common_thread_cores() {
        let num_cpus = std::env::var(ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
            .ok()
            .and_then(|v| v.parse::<usize>().ok().filter(|n| *n > 0))
            .unwrap_or(std::thread::available_parallelism().unwrap().get());
        assert!(num_cpus > 0);

        std::env::set_var(ENV_NACOS_CLIENT_COMMON_THREAD_CORES, "4");
        let num_cpus = std::env::var(ENV_NACOS_CLIENT_COMMON_THREAD_CORES)
            .ok()
            .and_then(|v| v.parse::<usize>().ok().filter(|n| *n > 0))
            .unwrap_or(std::thread::available_parallelism().unwrap().get());
        assert_eq!(num_cpus, 4);
    }

    #[test]
    fn test_spawn() {
        let handler = spawn(async {
            println!("test spawn task");
            5
        });
        let ret = RT.block_on(handler);
        let ret = ret.unwrap();
        assert_eq!(ret, 5);
    }

    #[test]
    fn test_schedule() {
        let handler = schedule(
            async move {
                println!("test schedule task");
                5
            },
            tokio::time::Duration::from_secs(1),
        );

        let ret = RT.block_on(handler);
        let ret = ret.unwrap();
        assert_eq!(ret, 5);
    }

    #[test]
    fn test_schedule_at_fixed_delay() {
        let handler = schedule_at_fixed_delay(
            || {
                println!("test schedule at fixed delay");
                Ok(())
            },
            tokio::time::Duration::from_secs(1),
        );

        std::thread::sleep(core::time::Duration::from_secs(3));
        handler.abort();
        std::thread::sleep(core::time::Duration::from_secs(5));
        println!("task has been canceled!")
    }

    #[test]
    fn test_schedule_at_fixed_rate() {
        let handler = schedule_at_fixed_rate(
            || {
                println!("test schedule at fixed rate");
                Ok(())
            },
            tokio::time::Duration::from_secs(1),
        );

        std::thread::sleep(core::time::Duration::from_secs(3));
        handler.abort();
        std::thread::sleep(core::time::Duration::from_secs(5));
        println!("task has been canceled!")
    }

    #[test]
    fn test_spawn_hundred_task() {
        for i in 1..100 {
            let _ = spawn(async move {
                println!("test_spawn_thousand_task spawn {i}");
            });
        }
        for j in 1..100 {
            let _ = schedule(
                async move {
                    println!("test_spawn_thousand_task schedule {j}");
                },
                Duration::from_millis(j),
            );
        }
        std::thread::sleep(Duration::from_millis(1010));
    }
}

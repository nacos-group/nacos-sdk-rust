use futures::Future;
use lazy_static::lazy_static;
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
    time::{interval, sleep, Duration},
};

use std::thread::available_parallelism;

lazy_static! {
    static ref RT: Runtime = Builder::new_multi_thread()
        .enable_all()
        .worker_threads(available_parallelism().unwrap().get() * 2 + 1)
        .thread_name("nacos-client-thread-pool")
        .build()
        .unwrap();
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RT.spawn(future)
}

pub fn schedule<F>(future: F, delay: Duration) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RT.spawn(async move {
        sleep(delay).await;
        future.await
    })
}

pub fn schedule_at_fixed_rate<Fut>(
    func: impl Fn() -> Option<Fut> + Send + 'static,
    duration: Duration,
) -> JoinHandle<()>
where
    Fut: Future<Output = ()> + Send + 'static,
{
    RT.spawn(async move {
        loop {
            let future = func();
            if future.is_none() {
                break;
            }
            let future = future.unwrap();
            future.await;
            sleep(duration).await;
        }
    })
}

pub fn schedule_at_fixed_delay<Fut>(
    func: impl Fn() -> Option<Fut> + Send + 'static,
    duration: Duration,
) -> JoinHandle<()>
where
    Fut: Future + Send + 'static,
{
    RT.spawn(async move {
        let mut interval = interval(duration);
        loop {
            interval.tick().await;
            let future = func();
            if future.is_none() {
                break;
            }
            let future = future.unwrap();
            future.await;
        }
    })
}

#[cfg(test)]
mod tests {

    use super::*;

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
                Some(async move {
                    println!("test schedule at fixed delay");
                })
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
                Some(async move {
                    println!("test schedule at fixed rate");
                })
            },
            tokio::time::Duration::from_secs(1),
        );

        std::thread::sleep(core::time::Duration::from_secs(3));
        handler.abort();
        std::thread::sleep(core::time::Duration::from_secs(5));
        println!("task has been canceled!")
    }
}

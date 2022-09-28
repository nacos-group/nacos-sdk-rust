use futures::Future;
use lazy_static::lazy_static;
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
    time::{interval, sleep, Duration},
};

lazy_static! {
    static ref RT: Runtime = Builder::new_multi_thread()
        .enable_all()
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

pub fn schedule<F>(future: F, duration: Duration) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RT.spawn(async move {
        sleep(duration).await;
        future.await
    })
}

pub fn schedule_at_fixed_rate<Fut>(
    func: impl Fn() -> Fut + Send + 'static,
    duration: Duration,
) -> JoinHandle<()>
where
    Fut: Future<Output = ()> + Send + 'static,
{
    RT.spawn(async move {
        loop {
            let future = func();
            future.await;
            sleep(duration).await;
        }
    })
}

pub fn schedule_at_fixed_delay<Fut>(
    func: impl Fn() -> Fut + Send + 'static,
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
            || async move {
                println!("test schedule at fixed delay");
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
            || async move {
                println!("test schedule at fixed rate");
            },
            tokio::time::Duration::from_secs(1),
        );

        std::thread::sleep(core::time::Duration::from_secs(3));
        handler.abort();
        std::thread::sleep(core::time::Duration::from_secs(5));
        println!("task has been canceled!")
    }
}

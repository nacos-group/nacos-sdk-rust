use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::{
    sync::RwLock,
    time::{self, sleep},
};
use tracing::{Instrument, debug, debug_span, instrument};

use crate::api::error::Result;
use crate::common::{executor, remote::grpc::NacosGrpcClient};

pub(crate) mod automatic_request;

pub(crate) struct RedoTaskExecutor {
    map: Arc<RwLock<HashMap<String, Arc<dyn RedoTask>>>>,
    id: String,
}

impl RedoTaskExecutor {
    pub(crate) fn new(id: String) -> Self {
        let executor = Self {
            id,
            map: Arc::new(RwLock::new(HashMap::new())),
        };
        executor.start_schedule();
        executor
    }

    fn start_schedule(&self) {
        let _span_enter = debug_span!("RedoTaskExecutor-task", id = self.id).entered();
        debug!("start schedule automatic request task.");
        let map = self.map.clone();
        executor::spawn(
            async move {
                sleep(Duration::from_millis(3000)).await;
                let mut interval = time::interval(Duration::from_millis(3000));
                loop {
                    interval.tick().await;

                    let active_tasks: Vec<Arc<dyn RedoTask>> = {
                        let map = map.read().await;
                        map.values()
                            .filter(|task| task.is_active())
                            .cloned()
                            .collect()
                    };
                    if !active_tasks.is_empty() {
                        debug!("automatic request task triggered!");
                    }
                    for task in active_tasks {
                        debug!("automatic request task: {:?}", task.task_key());
                        task.run().await;
                    }
                }
            }
            .in_current_span(),
        );
    }

    #[instrument(skip_all)]
    pub(crate) async fn add_task(&self, task: Arc<dyn RedoTask>) {
        let mut map = self.map.write().await;
        let task_key = task.task_key();
        map.insert(task_key, task);
    }

    #[instrument(fields(task_key = task_key), skip_all)]
    pub(crate) async fn remove_task(&self, task_key: &str) {
        let mut map = self.map.write().await;
        map.remove(task_key);
    }

    /// Whether a redo task is registered for the given key.
    ///
    /// The Java client uses the equivalent check (`isSubscribed`) to decide
    /// whether a cached service info can be served as-is or whether the
    /// subscription has to be re-established.
    #[instrument(fields(task_key = task_key), skip_all)]
    pub(crate) async fn contains_task(&self, task_key: &str) -> bool {
        let map = self.map.read().await;
        map.contains_key(task_key)
    }

    #[instrument(fields(client_id = &self.id), skip_all)]
    pub(crate) async fn on_grpc_client_reconnect(&self) {
        let map = self.map.read().await;
        for v in map.values() {
            v.active()
        }
    }

    #[instrument(fields(client_id = &self.id), skip_all)]
    pub(crate) async fn on_grpc_client_disconnect(&self) {
        let map = self.map.read().await;
        for v in map.values() {
            v.frozen()
        }
    }
}

#[async_trait]
pub(crate) trait RedoTask: Send + Sync + 'static {
    fn task_key(&self) -> String;

    fn frozen(&self);

    fn active(&self);

    fn is_active(&self) -> bool;

    async fn run(&self);
}

type CallBack = Box<dyn Fn(Result<()>) + Send + Sync + 'static>;

#[async_trait]
pub(crate) trait AutomaticRequest: Send + Sync + 'static {
    async fn run(&self, grpc_client: Arc<NacosGrpcClient>, call_back: CallBack);

    fn name(&self) -> String;
}

/// The canonical redo task key: `namespace@@group@@service@@identity`.
///
/// Both [`AutomaticRequest::name`] and the naming service's subscription
/// lookup build this key, so it is defined once here — the two must not be
/// able to drift apart, or the subscription state check silently stops
/// working.
pub(crate) fn redo_task_key(
    namespace: &str,
    group_name: &str,
    service_name: &str,
    identity: std::borrow::Cow<'_, str>,
) -> String {
    format!("{namespace}@@{group_name}@@{service_name}@@{identity}")
}
pub(crate) struct NamingRedoTask {
    active: Arc<AtomicBool>,
    automatic_request: Arc<dyn AutomaticRequest>,
    grpc_client: Arc<NacosGrpcClient>,
}

impl NamingRedoTask {
    pub(crate) fn new(
        grpc_client: Arc<NacosGrpcClient>,
        automatic_request: Arc<dyn AutomaticRequest>,
    ) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            automatic_request,
            grpc_client,
        }
    }
}

#[async_trait]
impl RedoTask for NamingRedoTask {
    fn task_key(&self) -> String {
        self.automatic_request.name()
    }

    fn frozen(&self) {
        self.active.store(false, Ordering::Release)
    }

    fn active(&self) {
        self.active.store(true, Ordering::Release)
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    #[instrument(skip_all)]
    async fn run(&self) {
        let active = self.active.clone();
        self.automatic_request
            .run(
                self.grpc_client.clone(),
                Box::new(move |ret| {
                    if ret.is_ok() {
                        active.store(false, Ordering::Release);
                    } else {
                        active.store(true, Ordering::Release);
                    }
                }),
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{Mutex, oneshot};

    struct BlockingRedoTask {
        started: Mutex<Option<oneshot::Sender<()>>>,
        release: Mutex<Option<oneshot::Receiver<()>>>,
    }

    #[async_trait]
    impl RedoTask for BlockingRedoTask {
        fn task_key(&self) -> String {
            "blocking".to_string()
        }

        fn frozen(&self) {}

        fn active(&self) {}

        fn is_active(&self) -> bool {
            true
        }

        async fn run(&self) {
            let started = { self.started.lock().await.take() };
            if let Some(tx) = started {
                let _ = tx.send(());
            }
            let release = { self.release.lock().await.take() };
            if let Some(rx) = release {
                let _ = rx.await;
            }
        }
    }

    #[tokio::test]
    async fn remove_task_does_not_wait_for_running_task() {
        let executor = RedoTaskExecutor::new("test".to_string());
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();

        executor
            .add_task(Arc::new(BlockingRedoTask {
                started: Mutex::new(Some(started_tx)),
                release: Mutex::new(Some(release_rx)),
            }))
            .await;

        tokio::time::timeout(Duration::from_secs(10), started_rx)
            .await
            .expect("redo task should be scheduled within 10s")
            .expect("redo task should signal start");

        let removed =
            tokio::time::timeout(Duration::from_millis(500), executor.remove_task("blocking"))
                .await;
        let _ = release_tx.send(());

        removed.expect("remove_task must not wait for a running redo task");
    }
}

use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use tokio::{
    sync::RwLock,
    time::{self, sleep},
};
use tonic::async_trait;
use tracing::{debug, debug_span, instrument};

use crate::api::{error::Result, plugin::AuthPlugin};
use crate::common::{executor, remote::grpc::NacosGrpcClient};

pub(crate) mod automatic_request;

pub(crate) struct RedoTaskExecutor {
    map: Arc<RwLock<HashMap<String, Arc<dyn RedoTask>>>>,
    client_id: String,
}

impl RedoTaskExecutor {
    pub(crate) fn new(auth_plugin: Arc<dyn AuthPlugin>, client_id: String) -> Self {
        let _redo_task_executor_span =
            debug_span!(parent: None, "redo_task_executor", client_id = client_id).entered();
        let executor = Self {
            map: Arc::new(RwLock::new(HashMap::new())),
            client_id,
        };
        executor.start_schedule();
        executor
    }

    fn start_schedule(&self) {
        debug!("start schedule automatic request task.");
        let map = self.map.clone();
        executor::spawn(async move {
            sleep(Duration::from_millis(3000)).await;
            let mut interval = time::interval(Duration::from_millis(3000));
            loop {
                interval.tick().await;

                let map = map.read().await;
                let active_tasks: Vec<Arc<dyn RedoTask>> = map
                    .iter()
                    .filter(|(_, v)| v.is_active())
                    .map(|(_, v)| v.clone())
                    .collect();
                if !active_tasks.is_empty() {
                    debug!("automatic request task triggered!");
                }
                for task in active_tasks {
                    debug!("automatic request task: {:?}", task.task_key());
                    task.run().await;
                }
            }
        });
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn add_task(&self, task: Arc<dyn RedoTask>) {
        let mut map = self.map.write().await;
        let task_key = task.task_key();
        map.insert(task_key, task);
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn remove_task(&self, task_key: &str) {
        let mut map = self.map.write().await;
        map.remove(task_key);
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn on_grpc_client_reconnect(&self) {
        let map = self.map.read().await;
        for (_, v) in map.iter() {
            v.active()
        }
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn on_grpc_client_disconnect(&self) {
        let map = self.map.read().await;
        for (_, v) in map.iter() {
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
        self.active
            .store(false, std::sync::atomic::Ordering::Release)
    }

    fn active(&self) {
        self.active
            .store(true, std::sync::atomic::Ordering::Release)
    }

    fn is_active(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::Acquire)
    }

    async fn run(&self) {
        let active = self.active.clone();
        self.automatic_request
            .run(
                self.grpc_client.clone(),
                Box::new(move |ret| {
                    if ret.is_ok() {
                        active.store(false, std::sync::atomic::Ordering::Release);
                    } else {
                        active.store(true, std::sync::atomic::Ordering::Release);
                    }
                }),
            )
            .await;
    }
}

use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use tokio::{
    sync::RwLock,
    time::{self, sleep},
};
use tracing::debug;

use crate::common::{executor, remote::grpc::NacosGrpcClient};
use crate::{
    api::{error::Result, plugin::AuthPlugin},
    common::remote::grpc::message::{GrpcRequestMessage, GrpcResponseMessage},
};

pub(crate) mod automatic_request;

pub(crate) struct RedoTaskExecutor {
    map: Arc<RwLock<HashMap<String, Arc<dyn RedoTask>>>>,
    automatic_request_invoker: Arc<AutomaticRequestInvoker>,
}

impl RedoTaskExecutor {
    pub(crate) fn new(
        nacos_grpc_client: Arc<NacosGrpcClient>,
        auth_plugin: Arc<dyn AuthPlugin>,
    ) -> Self {
        let executor = Self {
            map: Arc::new(RwLock::new(HashMap::new())),
            automatic_request_invoker: Arc::new(AutomaticRequestInvoker {
                nacos_grpc_client,
                auth_plugin,
            }),
        };
        executor.start_schedule();
        executor
    }

    fn start_schedule(&self) {
        debug!("start schedule automatic request task.");
        let map = self.map.clone();
        let invoker = self.automatic_request_invoker.clone();
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
                    task.run(invoker.clone());
                }
            }
        });
    }

    pub(crate) async fn add_task(&self, task: Arc<dyn RedoTask>) {
        let mut map = self.map.write().await;
        let task_key = task.task_key();
        let is_contain = map.contains_key(&task_key);
        if is_contain {
            return;
        }
        map.insert(task_key, task);
    }

    pub(crate) async fn remove_task(&self, task_key: &str) {
        let mut map = self.map.write().await;
        map.remove(task_key);
    }

    pub(crate) async fn on_grpc_client_disconnect(&self) {
        let map = self.map.read().await;
        for (_, v) in map.iter() {
            v.frozen()
        }
    }

    pub(crate) async fn on_grpc_client_reconnect(&self) {
        let map = self.map.read().await;
        for (_, v) in map.iter() {
            v.active()
        }
    }
}

pub(crate) trait RedoTask: Send + Sync + 'static {
    fn task_key(&self) -> String;

    fn frozen(&self);

    fn active(&self);

    fn is_active(&self) -> bool;

    fn run(&self, invoker: Arc<AutomaticRequestInvoker>);
}

type CallBack = Box<dyn Fn(Result<()>) + Send + Sync + 'static>;
pub(crate) trait AutomaticRequest: Send + Sync + 'static {
    fn run(&self, invoker: Arc<AutomaticRequestInvoker>, call_back: CallBack);

    fn name(&self) -> String;
}

pub(crate) struct AutomaticRequestInvoker {
    nacos_grpc_client: Arc<NacosGrpcClient>,
    auth_plugin: Arc<dyn AuthPlugin>,
}

impl AutomaticRequestInvoker {
    pub(crate) async fn invoke<R, P>(&self, mut request: R) -> Result<P>
    where
        R: GrpcRequestMessage + 'static,
        P: GrpcResponseMessage + 'static,
    {
        request.add_headers(self.auth_plugin.get_login_identity().contexts);
        self.nacos_grpc_client
            .unary_call_async::<R, P>(request)
            .await
    }
}

pub(crate) struct NamingRedoTask {
    active: Arc<AtomicBool>,
    automatic_request: Arc<dyn AutomaticRequest>,
}

impl NamingRedoTask {
    pub(crate) fn new(automatic_request: Arc<dyn AutomaticRequest>) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(false)),
            automatic_request,
        }
    }
}

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

    fn run(&self, invoker: Arc<AutomaticRequestInvoker>) {
        let active = self.active.clone();
        self.automatic_request.run(
            invoker,
            Box::new(move |ret| {
                if ret.is_ok() {
                    active.store(false, std::sync::atomic::Ordering::Release);
                } else {
                    active.store(true, std::sync::atomic::Ordering::Release);
                }
            }),
        );
    }
}

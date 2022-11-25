use std::sync::Arc;

use crate::{
    common::{
        event_bus::NacosEventSubscriber, executor, remote::grpc::events::GrpcDisconnectEvent,
    },
    naming::redo::RedoTaskExecutor,
};

pub(crate) struct RedoTaskDisconnectEventSubscriber {
    pub(crate) redo_task_executor: Arc<RedoTaskExecutor>,
}

impl NacosEventSubscriber for RedoTaskDisconnectEventSubscriber {
    type EventType = GrpcDisconnectEvent;

    fn on_event(&self, _: &Self::EventType) {
        let redo_task_executor = self.redo_task_executor.clone();
        executor::spawn(async move {
            redo_task_executor.on_grpc_client_disconnect().await;
        });
    }
}

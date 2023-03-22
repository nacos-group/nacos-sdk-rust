use std::sync::Arc;

use tracing::debug;

use crate::{
    common::{
        event_bus::NacosEventSubscriber, executor, remote::grpc::events::ClientInitCompleteEvent,
    },
    naming::redo::RedoTaskExecutor,
};

pub(crate) struct RedoTaskReconnectEventSubscriber {
    pub(crate) redo_task_executor: Arc<RedoTaskExecutor>,
}

impl NacosEventSubscriber for RedoTaskReconnectEventSubscriber {
    type EventType = ClientInitCompleteEvent;

    fn on_event(&self, _: &Self::EventType) {
        debug!("receive ClientInitCompleteEvent, notify redo task executor.");
        let redo_task_executor = self.redo_task_executor.clone();
        executor::spawn(async move {
            redo_task_executor.on_grpc_client_reconnect().await;
        });
    }
}

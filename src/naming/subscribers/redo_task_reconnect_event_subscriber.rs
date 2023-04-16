use std::sync::Arc;

use tracing::{warn, warn_span, Instrument};

use crate::{
    common::{
        event_bus::NacosEventSubscriber, executor, remote::grpc::events::ClientInitCompleteEvent,
    },
    naming::redo::RedoTaskExecutor,
};

pub(crate) struct RedoTaskReconnectEventSubscriber {
    pub(crate) redo_task_executor: Arc<RedoTaskExecutor>,
    pub(crate) scope: String,
}

impl NacosEventSubscriber for RedoTaskReconnectEventSubscriber {
    type EventType = ClientInitCompleteEvent;

    fn on_event(&self, _: &Self::EventType) {
        let _redo_task_reconnect_event_subscriber_span = warn_span!(
            parent: None,
            "redo_task_reconnect_event_subscriber",
            client_id = self.scope
        )
        .entered();
        warn!("receive ClientInitCompleteEvent, notify redo task executor.");
        let redo_task_executor = self.redo_task_executor.clone();
        executor::spawn(
            async move {
                redo_task_executor.on_grpc_client_reconnect().await;
            }
            .in_current_span(),
        );
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

use std::sync::Arc;

use tracing::{warn, warn_span, Instrument};

use crate::{
    common::{event_bus::NacosEventSubscriber, executor, remote::grpc::events::DisconnectEvent},
    naming::redo::RedoTaskExecutor,
};

pub(crate) struct RedoTaskDisconnectEventSubscriber {
    pub(crate) redo_task_executor: Arc<RedoTaskExecutor>,
    pub(crate) scope: String,
}

impl NacosEventSubscriber for RedoTaskDisconnectEventSubscriber {
    type EventType = DisconnectEvent;

    fn on_event(&self, _: &Self::EventType) {
        let _redo_task_disconnect_event_subscriber_span = warn_span!(
            parent: None,
            "redo_task_disconnect_event_subscriber",
            client_id = self.scope
        )
        .entered();

        warn!("receive DisconnectEvent, notify redo task executor.");
        let redo_task_executor = self.redo_task_executor.clone();
        executor::spawn(
            async move {
                redo_task_executor.on_grpc_client_disconnect().await;
            }
            .in_current_span(),
        );
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

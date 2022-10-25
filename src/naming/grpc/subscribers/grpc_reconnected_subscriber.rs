use std::sync::Arc;

use tracing::info;

use crate::{
    api::events::{HandEventFuture, NacosEventSubscriber},
    naming::grpc::{events::GrpcReconnectedEvent, grpc_service::ServerSetUP, GrpcService},
};

pub struct GrpcReconnectedEventSubscriber {
    pub grpc_service: Arc<GrpcService>,
    pub set_up_info: ServerSetUP,
}

impl NacosEventSubscriber for GrpcReconnectedEventSubscriber {
    type EventType = GrpcReconnectedEvent;

    fn on_event(&self, _: &Self::EventType) -> Option<HandEventFuture> {
        info!("receive grpc reconnect event.");

        let grpc_service = self.grpc_service.clone();
        let set_up_info = self.set_up_info.clone();
        let task = async move {
            // init
            grpc_service.init(set_up_info).await;

            Ok(())
        };

        Some(Box::new(Box::pin(task)))
    }
}

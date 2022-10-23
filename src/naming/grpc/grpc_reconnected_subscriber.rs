use std::sync::Arc;

use tracing::{error, info};

use crate::api::events::{common::GrpcReconnectedEvent, HandEventFuture, NacosEventSubscriber};

use super::{grpc_service::ServerSetUP, GrpcService};

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
            // check server
            let check_server_rsp = grpc_service.check_server().await;
            if let Err(e) = check_server_rsp {
                error!("check server error. {:?}", e);
                return Err(e);
            }

            // set connection id
            let check_server_rsp = check_server_rsp.unwrap();
            let connection_id = check_server_rsp.connection_id;
            let mut current_connection_id = grpc_service.connection_id.lock().await;
            *current_connection_id = connection_id;

            // set up
            grpc_service.setup(set_up_info.clone()).await;
            Ok(())
        };

        Some(Box::new(Box::pin(task)))
    }
}

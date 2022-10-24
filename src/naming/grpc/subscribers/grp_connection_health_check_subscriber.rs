use std::sync::Arc;

use tracing::{error, info};

use crate::{
    api::events::{HandEventFuture, NacosEventSubscriber},
    naming::grpc::{
        events::GrpcConnectHealthCheckEvent,
        message::{request::HealthCheckRequest, response::HealthCheckResponse},
        GrpcService,
    },
};

pub struct GrpcConnectHealthCheckEventSubscriber {
    pub grpc_service: Arc<GrpcService>,
}

impl NacosEventSubscriber for GrpcConnectHealthCheckEventSubscriber {
    type EventType = GrpcConnectHealthCheckEvent;

    fn on_event(&self, _: &Self::EventType) -> Option<HandEventFuture> {
        info!("receive grpc connection health check event.");

        let grpc_service = self.grpc_service.clone();

        let task = async move {
            // health check
            let health_check_request = HealthCheckRequest {
                ..Default::default()
            };

            let ret = grpc_service
                .unary_call_async::<HealthCheckRequest, HealthCheckResponse>(health_check_request)
                .await;
            if let Err(e) = ret {
                error!("health check failed. {:?}", e);
            }

            Ok(())
        };

        Some(Box::new(Box::pin(task)))
    }
}

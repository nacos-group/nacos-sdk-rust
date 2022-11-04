use std::sync::Arc;
use tracing::{debug, error};

use crate::{
    api::events::NacosEventSubscriber,
    common::{
        executor,
        remote::grpc::{
            message::{request::HealthCheckRequest, response::HealthCheckResponse},
            NacosGrpcClient,
        },
    },
};

use crate::common::remote::grpc::events::GrpcConnectHealthCheckEvent;

pub(crate) struct GrpcConnectHealthCheckEventSubscriber {
    pub nacos_grpc_client: Arc<NacosGrpcClient>,
}

impl NacosEventSubscriber for GrpcConnectHealthCheckEventSubscriber {
    type EventType = GrpcConnectHealthCheckEvent;

    fn on_event(&self, _: &Self::EventType) {
        debug!("received grpc connection health check event.");

        let nacos_grpc_client = self.nacos_grpc_client.clone();
        executor::spawn(async move {
            let health_check_request = HealthCheckRequest {
                ..Default::default()
            };

            let ret = nacos_grpc_client
                .unary_call_async::<HealthCheckRequest, HealthCheckResponse>(health_check_request)
                .await;
            if let Err(e) = ret {
                error!("grpc connection health check failed: {:?}", e);
            }
        });
    }
}

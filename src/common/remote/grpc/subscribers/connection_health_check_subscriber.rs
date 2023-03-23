use std::sync::Arc;
use tracing::{debug, error};

use crate::common::{
    event_bus::NacosEventSubscriber,
    executor,
    remote::grpc::{
        message::{request::HealthCheckRequest, response::HealthCheckResponse},
        NacosGrpcClient,
    },
};

use crate::common::remote::grpc::events::ConnectionHealthCheckEvent;

pub(crate) struct ConnectionHealthCheckEventSubscriber {
    pub nacos_grpc_client: Arc<NacosGrpcClient>,
}

impl NacosEventSubscriber for ConnectionHealthCheckEventSubscriber {
    type EventType = ConnectionHealthCheckEvent;

    fn on_event(&self, _: &Self::EventType) {
        debug!("received ConnectionHealthCheckEvent.");

        let nacos_grpc_client = self.nacos_grpc_client.clone();
        executor::spawn(async move {
            let health_check_request = HealthCheckRequest {
                ..Default::default()
            };

            let ret = nacos_grpc_client
                .unary_call_async::<HealthCheckRequest, HealthCheckResponse>(health_check_request)
                .await;
            if let Err(e) = ret {
                error!("connection health check failed: {e:?}");
            }
        });
    }
}

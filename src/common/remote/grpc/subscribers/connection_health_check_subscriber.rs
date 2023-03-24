use std::sync::Arc;
use tracing::{debug, debug_span, error, Instrument};

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
    pub scope: String,
}

impl NacosEventSubscriber for ConnectionHealthCheckEventSubscriber {
    type EventType = ConnectionHealthCheckEvent;

    fn on_event(&self, _: &Self::EventType) {
        let _connection_health_check_event_subscriber_span = debug_span!(
            parent: None,
            "connection_health_check_event_subscriber",
            client_id = self.scope
        )
        .entered();
        debug!("received ConnectionHealthCheckEvent.");

        let nacos_grpc_client = self.nacos_grpc_client.clone();
        executor::spawn(
            async move {
                let health_check_request = HealthCheckRequest {
                    ..Default::default()
                };

                let ret = nacos_grpc_client
                    .unary_call_async::<HealthCheckRequest, HealthCheckResponse>(
                        health_check_request,
                    )
                    .await;
                if let Err(e) = ret {
                    error!("connection health check failed: {e:?}");
                }
            }
            .in_current_span(),
        );
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

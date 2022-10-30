use crate::api::events::NacosEvent;

#[derive(Clone, Debug)]
pub struct GrpcConnectHealthCheckEvent {}

impl NacosEvent for GrpcConnectHealthCheckEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "GrpcConnectHealthCheckEvent".to_string()
    }
}

use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub struct GrpcReconnectedEvent {}

impl NacosEvent for GrpcReconnectedEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "GrpcReconnectedEvent".to_string()
    }
}

use crate::api::events::NacosEvent;

#[derive(Clone, Debug)]
pub struct GrpcDisconnectEvent {}

impl NacosEvent for GrpcDisconnectEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "GrpcDisconnectEvent".to_string()
    }
}

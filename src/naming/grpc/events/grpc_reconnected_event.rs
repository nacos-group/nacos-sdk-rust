use crate::api::events::NacosEvent;

#[derive(Clone, Debug)]
pub struct GrpcReconnectedEvent {}

impl NacosEvent for GrpcReconnectedEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

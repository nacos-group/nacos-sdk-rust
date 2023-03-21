use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct DisconnectEvent;

impl NacosEvent for DisconnectEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "DisconnectEvent".to_string()
    }
}

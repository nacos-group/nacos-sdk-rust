use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ReconnectedEvent;

impl NacosEvent for ReconnectedEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "ReconnectedEvent".to_string()
    }
}

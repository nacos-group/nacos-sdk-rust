use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ShutdownEvent;

impl NacosEvent for ShutdownEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "ShutdownEvent".to_string()
    }
}

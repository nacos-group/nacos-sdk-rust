use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ShutdownEvent {
    pub(crate) scope: String,
}

impl NacosEvent for ShutdownEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> &str {
        "ShutdownEvent"
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

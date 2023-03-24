use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ReconnectedEvent {
    pub(crate) scope: String,
}

impl NacosEvent for ReconnectedEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> &str {
        "ReconnectedEvent"
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

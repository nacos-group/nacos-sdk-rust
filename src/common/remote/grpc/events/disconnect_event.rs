use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct DisconnectEvent {
    pub(crate) scope: String,
}

impl NacosEvent for DisconnectEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> &str {
        "DisconnectEvent"
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ConnectionHealthCheckEvent {
    pub(crate) scope: String,
}

impl NacosEvent for ConnectionHealthCheckEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> &str {
        "ConnectionHealthCheckEvent"
    }

    fn scope(&self) -> &str {
        &self.scope
    }
}

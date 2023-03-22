use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ConnectionHealthCheckEvent;

impl NacosEvent for ConnectionHealthCheckEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "ConnectionHealthCheckEvent".to_string()
    }
}

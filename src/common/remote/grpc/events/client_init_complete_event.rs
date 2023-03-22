use std::any::Any;

use crate::common::event_bus::NacosEvent;

#[derive(Clone, Debug)]
pub(crate) struct ClientInitCompleteEvent;

impl NacosEvent for ClientInitCompleteEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn Any
    }

    fn event_identity(&self) -> String {
        "ClientInitCompleteEvent".to_string()
    }
}

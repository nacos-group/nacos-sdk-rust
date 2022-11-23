use std::any::Any;

use crate::api::events::NacosEvent;

#[derive(Clone, Debug)]
pub struct NacosGrpcClientInitComplete {}

impl NacosEvent for NacosGrpcClientInitComplete {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn Any
    }

    fn event_identity(&self) -> String {
        "NacosGrpcClientInitComplete".to_string()
    }
}

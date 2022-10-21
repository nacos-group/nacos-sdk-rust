use crate::api::{events::NacosEvent, naming::ServiceInstance};

#[derive(Clone, Debug)]
pub struct InstancesChangeEvent {
    pub event_scope: String,
    pub service_name: String,
    pub group_name: String,
    pub clusters: String,
    pub hosts: Option<Vec<ServiceInstance>>,
}

impl NacosEvent for InstancesChangeEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

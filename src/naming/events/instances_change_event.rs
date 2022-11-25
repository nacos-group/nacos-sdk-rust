use std::sync::Arc;

use crate::api::naming::ServiceInstance;
use crate::common::event_bus::NacosEvent;
use crate::naming::dto::ServiceInfo;

#[derive(Clone, Debug)]
pub struct InstancesChangeEvent {
    event_scope: String,
    service_info: Arc<ServiceInfo>,
}

impl InstancesChangeEvent {
    pub fn new(event_scope: String, service_info: Arc<ServiceInfo>) -> Self {
        Self {
            event_scope,
            service_info,
        }
    }

    pub fn event_scope(&self) -> &str {
        &self.event_scope
    }

    pub fn service_name(&self) -> &str {
        &self.service_info.name
    }

    pub fn group_name(&self) -> &str {
        &self.service_info.group_name
    }

    pub fn clusters(&self) -> &str {
        &self.service_info.clusters
    }

    pub fn hosts(&self) -> Option<&Vec<ServiceInstance>> {
        self.service_info.hosts.as_ref()
    }

    pub fn service_info(&self) -> Arc<ServiceInfo> {
        self.service_info.clone()
    }
}

impl NacosEvent for InstancesChangeEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn event_identity(&self) -> String {
        "InstancesChangeEvent".to_string()
    }
}

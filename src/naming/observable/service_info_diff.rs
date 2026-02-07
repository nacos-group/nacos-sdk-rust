use tracing::info;

use crate::{api::naming::ServiceInstance, naming::dto::ServiceInfo};

/// Represents the difference between two service info snapshots
#[derive(Debug, Clone, Default)]
pub struct ServiceInfoDiff<'a> {
    pub new_instances: Vec<&'a ServiceInstance>,
    pub removed_instances: Vec<&'a ServiceInstance>,
    pub modified_instances: Vec<&'a ServiceInstance>,
    pub changed: bool,
}

impl<'a> ServiceInfoDiff<'a> {
    /// Calculate the difference between old and new service hosts
    pub fn calculate(old_hosts: &'a [ServiceInstance], new_hosts: &'a [ServiceInstance]) -> Self {
        let old_hosts_map: std::collections::HashMap<String, &ServiceInstance> = old_hosts
            .iter()
            .map(|host| (host.ip_and_port(), host))
            .collect();
        let new_hosts_map: std::collections::HashMap<String, &ServiceInstance> = new_hosts
            .iter()
            .map(|host| (host.ip_and_port(), host))
            .collect();

        let mut diff = Self::default();

        // Find new and modified instances
        for (key, new_host) in new_hosts_map.iter() {
            match old_hosts_map.get(key) {
                None => diff.new_instances.push(*new_host),
                Some(old_host) => {
                    if !old_host.is_same_instance(new_host) {
                        diff.modified_instances.push(*new_host);
                    }
                }
            }
        }

        // Find removed instances
        for (key, old_host) in old_hosts_map.iter() {
            if !new_hosts_map.contains_key(key) {
                diff.removed_instances.push(*old_host);
            }
        }

        diff.changed = !diff.new_instances.is_empty()
            || !diff.removed_instances.is_empty()
            || !diff.modified_instances.is_empty();

        diff
    }

    /// Log the changes for debugging purposes
    pub fn log_changes(&self, key: &str) {
        if !self.new_instances.is_empty() {
            let json = serde_json::to_string(&self.new_instances).unwrap_or("[]".to_owned());
            info!(
                "new ips({}) service: {} -> {}",
                self.new_instances.len(),
                key,
                json
            );
        }

        if !self.removed_instances.is_empty() {
            let json = serde_json::to_string(&self.removed_instances).unwrap_or("[]".to_owned());
            info!(
                "removed ips({}) service: {} -> {}",
                self.removed_instances.len(),
                key,
                json
            );
        }

        if !self.modified_instances.is_empty() {
            let json = serde_json::to_string(&self.modified_instances).unwrap_or("[]".to_owned());
            info!(
                "modified ips({}) service: {} -> {}",
                self.modified_instances.len(),
                key,
                json
            );
        }
    }
}

/// Check if the new service info is outdated compared to cached data
pub fn is_outdated_data(old_service: &ServiceInfo, new_service: &ServiceInfo) -> bool {
    old_service.last_ref_time > new_service.last_ref_time
}

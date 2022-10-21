use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tracing::{error, info, warn};

use crate::{
    api::{events::naming::InstancesChangeEvent, naming::ServiceInstance},
    common::event_bus,
};

use super::dto::ServiceInfo;

mod disk_cache;

pub(crate) struct ServiceInfoHolder {
    service_info_map: Arc<Mutex<HashMap<String, Arc<ServiceInfo>>>>,

    push_empty_protection: bool,

    cache_dir: String,

    notifier_event_scope: String,
}

impl ServiceInfoHolder {
    pub fn process_service_info(&self, service_info: Arc<ServiceInfo>) {
        if self.is_empty_or_error_push(&service_info) {
            return;
        }

        let name = service_info.get_grouped_service_name();
        let key = ServiceInfo::get_key(&name, &service_info.clusters);

        let lock = self.service_info_map.lock();
        if let Err(e) = lock {
            error!(
                "process push message from the server side failed, can't get lock. {:?}",
                e
            );
            return;
        }

        let mut map = lock.unwrap();

        let old_service = map.get(&key);
        let changed = Self::is_changed_service_info(old_service, &service_info);

        if changed {
            info!(
                "current ips:({}) service: {} -> {}",
                service_info.ip_count(),
                key,
                service_info.hosts_to_json()
            );

            let instance_change_event = InstancesChangeEvent {
                event_scope: self.notifier_event_scope.clone(),
                service_name: service_info.name.clone(),
                clusters: service_info.clusters.clone(),
                group_name: service_info.group_name.clone(),
                hosts: service_info.hosts.clone(),
            };
            event_bus::post(Box::new(instance_change_event));
            disk_cache::write(service_info.clone(), self.cache_dir.clone());
        }

        map.insert(key, service_info);
    }

    fn is_changed_service_info(
        old_service: Option<&Arc<ServiceInfo>>,
        new_service: &Arc<ServiceInfo>,
    ) -> bool {
        let name = new_service.get_grouped_service_name();
        let key = ServiceInfo::get_key(&name, &new_service.clusters);
        let hosts_json = new_service.hosts_to_json();

        if old_service.is_none() {
            let ip_count = new_service.ip_count();
            info!(
                "init new ips({}) service: {} -> {}",
                ip_count, key, hosts_json
            );
            return true;
        }

        let old_service = old_service.unwrap();

        if old_service.last_ref_time > new_service.last_ref_time {
            warn!(
                "out of date data received, old-t: {}, new-t: {}",
                old_service.last_ref_time, new_service.last_ref_time
            );
            return false;
        }

        let old_hosts = old_service.hosts.as_ref();
        let new_hosts = old_service.hosts.as_ref();

        if new_hosts.is_none() && old_hosts.is_none() {
            return false;
        }

        if new_hosts.is_none() || old_hosts.is_none() {
            return true;
        }

        let old_hosts = old_hosts.unwrap();
        let new_hosts = new_hosts.unwrap();

        let new_hosts_map: HashMap<String, &ServiceInstance> = new_hosts
            .iter()
            .map(|hosts| (hosts.ip_and_port(), hosts))
            .collect();
        let old_hosts_map: HashMap<String, &ServiceInstance> = old_hosts
            .iter()
            .map(|hosts| (hosts.ip_and_port(), hosts))
            .collect();

        let mut changed = false;

        let mut modified_hosts = Vec::<&ServiceInstance>::new();
        let mut new_add_hosts = Vec::<&ServiceInstance>::new();
        let mut removed_hosts = Vec::<&ServiceInstance>::new();

        for (key, new_host) in new_hosts_map.iter() {
            let old_host = old_hosts_map.get(key);
            if old_host.is_none() {
                new_add_hosts.push(*new_host);
                continue;
            }

            let old_host = old_host.unwrap();
            if !old_host.is_same_instance(*new_host) {
                modified_hosts.push(*new_host);
            }
        }

        for (key, old_host) in old_hosts_map.iter() {
            let new_host = new_hosts_map.get(key);
            if new_host.is_none() {
                removed_hosts.push(*old_host);
            }
        }

        if !new_add_hosts.is_empty() {
            info!(
                "new ips({}) service: {} -> {}",
                new_add_hosts.len(),
                key,
                hosts_json
            );
            changed = true;
        }

        if !removed_hosts.is_empty() {
            info!(
                "removed ips({}) service: {} -> {}",
                removed_hosts.len(),
                key,
                hosts_json
            );
            changed = true;
        }

        if !modified_hosts.is_empty() {
            info!(
                "modified ips({}) service: {} -> {}",
                modified_hosts.len(),
                key,
                hosts_json
            );
            changed = true;
        }

        changed
    }

    fn is_empty_or_error_push(&self, service_info: &Arc<ServiceInfo>) -> bool {
        service_info.hosts.is_none() || (self.push_empty_protection && !service_info.validate())
    }
}

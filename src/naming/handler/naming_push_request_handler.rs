use std::collections::HashMap;
use std::sync::Arc;

use crate::api::naming::ServiceInstance;
use crate::common::remote::grpc::bi_channel::ResponseWriter;
use crate::common::remote::grpc::handler::GrpcPayloadHandler;
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::naming::dto::ServiceInfo;
use crate::naming::events::InstancesChangeEvent;

use serde::Serialize;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::common::event_bus;
use crate::{
    common::executor,
    nacos_proto::v2::Payload,
    naming::message::{request::NotifySubscriberRequest, response::NotifySubscriberResponse},
};

pub(crate) struct NamingPushRequestHandler {
    service_info_holder: Arc<ServiceInfoHolder>,
}

impl NamingPushRequestHandler {
    pub(crate) fn new(service_info_holder: Arc<ServiceInfoHolder>) -> Self {
        Self {
            service_info_holder,
        }
    }
}

impl GrpcPayloadHandler for NamingPushRequestHandler {
    fn hand(&self, response_writer: ResponseWriter, payload: Payload) {
        executor::spawn(async move {
            let response = NotifySubscriberResponse::ok();
            let grpc_message = GrpcMessageBuilder::new(response).build();
            let payload = grpc_message.into_payload();
            if let Err(e) = payload {
                error!(
                    "occur an error when handing NotifySubscriberRequest. {:?}",
                    e
                );
                return;
            }
            let payload = payload.unwrap();

            let ret = response_writer.write(payload).await;
            if let Err(e) = ret {
                error!("bi_sender send grpc message to server error. {:?}", e);
            }
        });
        let request = GrpcMessage::<NotifySubscriberRequest>::from_payload(payload);
        if let Err(e) = request {
            error!("convert payload to NotifySubscriberRequest error. {:?}", e);
            return;
        }

        let request = request.unwrap();

        let body = request.into_body();

        debug!(
            "receive NotifySubscriberRequest from nacos server: {:?}",
            body
        );

        let service_info = body.service_info;

        let service_info_holder = self.service_info_holder.clone();

        executor::spawn(async move {
            service_info_holder.process_service_info(service_info).await;
        });
    }
}

pub(crate) struct ServiceInfoHolder {
    service_info_map: Mutex<HashMap<String, Arc<ServiceInfo>>>,
    push_empty_protection: bool,
    event_scope: String,
}

impl ServiceInfoHolder {
    pub(crate) fn new(event_scope: String) -> Self {
        Self {
            service_info_map: Mutex::new(HashMap::new()),
            push_empty_protection: true,
            event_scope,
        }
    }

    pub(crate) async fn process_service_info(&self, service_info: ServiceInfo) {
        if self.is_empty_or_error_push(&service_info) {
            return;
        }

        let service_info = Arc::new(service_info);

        let name =
            ServiceInfo::get_grouped_service_name(&service_info.name, &service_info.group_name);
        let key = ServiceInfo::get_key(&name, &service_info.clusters);

        let mut map = self.service_info_map.lock().await;

        let old_service = map.get(&key);

        let changed = Self::is_changed_service_info(old_service, &service_info);

        if changed {
            info!(
                "current ips:({}) service: {} -> {}",
                service_info.ip_count(),
                key,
                service_info.hosts_to_json()
            );
            let event = Arc::new(InstancesChangeEvent::new(
                self.event_scope.clone(),
                service_info.clone(),
            ));
            event_bus::post(event);
        }
        map.insert(key, service_info);
    }

    fn is_changed_service_info(
        old_service: Option<&Arc<ServiceInfo>>,
        new_service: &ServiceInfo,
    ) -> bool {
        let name =
            ServiceInfo::get_grouped_service_name(&new_service.name, &new_service.group_name);
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
        let new_hosts = new_service.hosts.as_ref();

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
            if !old_host.is_same_instance(new_host) {
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
            let new_add_hosts_json = Self::vec_2_string::<&ServiceInstance>(new_add_hosts.as_ref());

            info!(
                "new ips({}) service: {} -> {}",
                new_add_hosts.len(),
                key,
                new_add_hosts_json
            );
            changed = true;
        }

        if !removed_hosts.is_empty() {
            let removed_hosts_json = Self::vec_2_string::<&ServiceInstance>(removed_hosts.as_ref());
            info!(
                "removed ips({}) service: {} -> {}",
                removed_hosts.len(),
                key,
                removed_hosts_json
            );
            changed = true;
        }

        if !modified_hosts.is_empty() {
            let modified_hosts_json =
                Self::vec_2_string::<&ServiceInstance>(modified_hosts.as_ref());
            info!(
                "modified ips({}) service: {} -> {}",
                modified_hosts.len(),
                key,
                modified_hosts_json
            );
            changed = true;
        }

        changed
    }

    fn vec_2_string<T: Serialize>(vec: &Vec<T>) -> String {
        match serde_json::to_string::<Vec<T>>(vec) {
            Ok(json) => json,
            Err(e) => {
                warn!(
                    "vec to json string error, it will return default value '[]', {:?}",
                    e
                );
                "[]".to_string()
            }
        }
    }

    fn is_empty_or_error_push(&self, service_info: &ServiceInfo) -> bool {
        service_info.hosts.is_none() || (self.push_empty_protection && !service_info.validate())
    }

    pub(crate) async fn get_service_info(
        &self,
        group_name: &str,
        service_name: &str,
        cluster_str: &str,
    ) -> Option<ServiceInfo> {
        let grouped_name = ServiceInfo::get_grouped_service_name(service_name, group_name);
        let key = ServiceInfo::get_key(&grouped_name, cluster_str);

        let map = self.service_info_map.lock().await;
        let ret = map.get(&key).map(|data| data.as_ref().clone());

        ret
    }
}

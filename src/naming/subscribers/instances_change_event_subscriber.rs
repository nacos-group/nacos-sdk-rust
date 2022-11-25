use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::api::naming::{NamingEvent, NamingEventListener};
use crate::common::{event_bus::NacosEventSubscriber, executor};
use crate::naming::dto::ServiceInfo;
use crate::naming::events::InstancesChangeEvent;

type ListenerMap = Arc<RwLock<HashMap<String, Vec<Arc<dyn NamingEventListener>>>>>;
pub(crate) struct InstancesChangeEventSubscriber {
    listener_map: ListenerMap,
    event_scope: String,
}

impl InstancesChangeEventSubscriber {
    pub(crate) fn new(event_scope: String) -> Self {
        Self {
            listener_map: Arc::new(RwLock::new(HashMap::new())),
            event_scope,
        }
    }

    pub(crate) async fn add_listener(
        &self,
        group_name: &str,
        service_name: &str,
        cluster_str: &str,
        listener: Arc<dyn NamingEventListener>,
    ) {
        let grouped_name = ServiceInfo::get_grouped_service_name(service_name, group_name);
        let key = ServiceInfo::get_key(&grouped_name, cluster_str);
        let mut map = self.listener_map.write().await;
        let listeners = map.get_mut(&key);
        if listeners.is_none() {
            let listeners = vec![listener];
            map.insert(key, listeners);
        } else {
            let listeners = listeners.unwrap();
            let index = Self::index_of_listener(listeners, &listener);
            if let Some(index) = index {
                listeners.remove(index);
            }
            listeners.push(listener);
        }
    }

    pub(crate) async fn remove_listener(
        &self,
        group_name: &str,
        service_name: &str,
        cluster_str: &str,
        listener: Arc<dyn NamingEventListener>,
    ) {
        let grouped_name = ServiceInfo::get_grouped_service_name(service_name, group_name);
        let key = ServiceInfo::get_key(&grouped_name, cluster_str);
        let mut map = self.listener_map.write().await;

        let listeners = map.get_mut(&key);
        if listeners.is_none() {
            return;
        }

        let listeners = listeners.unwrap();

        let index = Self::index_of_listener(listeners, &listener);
        if index.is_none() {
            return;
        }

        let index = index.unwrap();
        listeners.remove(index);
    }
}

impl InstancesChangeEventSubscriber {
    fn index_of_listener(
        vec: &[Arc<dyn NamingEventListener>],
        target: &Arc<dyn NamingEventListener>,
    ) -> Option<usize> {
        for (index, subscriber) in vec.iter().enumerate() {
            let subscriber_trait_ptr = subscriber.as_ref() as *const dyn NamingEventListener;
            let (subscriber_data_ptr, _): (*const u8, *const u8) =
                unsafe { std::mem::transmute(subscriber_trait_ptr) };

            let target_trait_ptr = target.as_ref() as *const dyn NamingEventListener;
            let (target_data_ptr, _): (*const u8, *const u8) =
                unsafe { std::mem::transmute(target_trait_ptr) };

            if subscriber_data_ptr == target_data_ptr {
                return Some(index);
            }
        }
        None
    }
}

impl NacosEventSubscriber for InstancesChangeEventSubscriber {
    type EventType = InstancesChangeEvent;

    fn on_event(&self, event: &Self::EventType) {
        if self.event_scope != event.event_scope() {
            return;
        }

        let service_info = event.service_info();
        let listener_map = self.listener_map.clone();
        executor::spawn(async move {
            let grouped_name =
                ServiceInfo::get_grouped_service_name(&service_info.name, &service_info.group_name);
            let key = ServiceInfo::get_key(&grouped_name, &service_info.clusters);
            let map = listener_map.read().await;
            let listeners = map.get(&key);
            if listeners.is_none() {
                return;
            }
            let listeners = listeners.unwrap();

            let event = Arc::new(NamingPushEvent {
                service_info: service_info.clone(),
            });

            for listener in listeners {
                let event = event.clone();
                let listener = listener.clone();
                executor::spawn(async move { listener.event(event) });
            }
        });
    }
}

#[derive(Clone, Debug)]
struct NamingPushEvent {
    service_info: Arc<ServiceInfo>,
}

impl NamingEvent for NamingPushEvent {
    fn service_name(&self) -> &str {
        &self.service_info.name
    }

    fn group_name(&self) -> &str {
        &self.service_info.group_name
    }

    fn clusters(&self) -> &str {
        &self.service_info.clusters
    }

    fn instances(&self) -> Option<&Vec<crate::api::naming::ServiceInstance>> {
        self.service_info.hosts.as_ref()
    }
}

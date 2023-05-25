use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    RwLock,
};
use tracing::{debug_span, error, info, instrument, warn, Instrument, Span};

use crate::{
    api::naming::{NamingChangeEvent, NamingEventListener, ServiceInstance},
    common::{
        cache::{Cache, CacheRef},
        executor,
    },
    naming::dto::ServiceInfo,
};

pub(crate) fn create(
    id: String,
    cache: Arc<Cache<ServiceInfo>>,
    push_empty_protection: bool,
) -> (ServiceInfoObserver, Arc<ServiceInfoEmitter>) {
    let (sender, receiver) = channel::<(ServiceInfo, Span)>(1024);
    let service_info_observer = ServiceInfoObserver::new(id, receiver);
    let service_info_emitter = ServiceInfoEmitter::new(sender, cache, push_empty_protection);
    (service_info_observer, Arc::new(service_info_emitter))
}

type ListenerRegistry = Arc<RwLock<HashMap<String, Vec<Arc<dyn NamingEventListener>>>>>;
pub(crate) struct ServiceInfoObserver {
    registry: ListenerRegistry,
}

impl ServiceInfoObserver {
    fn new(id: String, receiver: Receiver<(ServiceInfo, Span)>) -> Self {
        let registry: ListenerRegistry = Default::default();
        executor::spawn(
            ServiceInfoObserver::observe(receiver, registry.clone())
                .instrument(debug_span!("ServiceInfoObserver", id = id)),
        );
        Self { registry }
    }
}

impl ServiceInfoObserver {
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

impl ServiceInfoObserver {
    async fn observe(mut receiver: Receiver<(ServiceInfo, Span)>, registry: ListenerRegistry) {
        info!("service info observe task start!");
        while let Some((service_info, span)) = receiver.recv().await {
            let grouped_name =
                ServiceInfo::get_grouped_service_name(&service_info.name, &service_info.group_name);
            let key = ServiceInfo::get_key(&grouped_name, &service_info.clusters);

            let map = registry.read().await;
            let listeners = map.get(&key);
            if listeners.is_none() {
                warn!("the key {key:?} is not subscribed.");
                continue;
            }
            let listeners = listeners.unwrap();
            if listeners.is_empty() {
                warn!("the subscriber listener set of key {key:?} is empty.");
                continue;
            }

            let naming_event = NamingChangeEvent {
                service_name: service_info.name,
                group_name: service_info.group_name,
                clusters: service_info.clusters,
                instances: service_info.hosts,
            };

            let naming_event = Arc::new(naming_event);

            for listener in listeners {
                let naming_event = naming_event.clone();
                let listener = listener.clone();
                info!("notify listener: {key:?}, notify data: {naming_event:?}");
                executor::spawn(
                    async move { listener.event(naming_event) }.instrument(span.clone()),
                );
            }
        }
        info!("service info observe task quit!");
    }

    #[instrument(fields(subscribe_key = key), skip_all)]
    pub(crate) async fn subscribe(&self, key: String, listener: Arc<dyn NamingEventListener>) {
        info!("subscribe {key:?}");
        let mut map = self.registry.write().await;
        let listeners = map.get_mut(&key);
        if let Some(listeners) = listeners {
            let index = Self::index_of_listener(listeners, &listener);
            if let Some(index) = index {
                warn!("listener has already exist, remove old listener and then add new listener.");
                listeners.remove(index);
            }
            listeners.push(listener);
        } else {
            let listeners = vec![listener];
            map.insert(key, listeners);
        }
    }

    #[instrument(fields(unsubscribe_key = key), skip_all)]
    pub(crate) async fn unsubscribe(&self, key: String, listener: Arc<dyn NamingEventListener>) {
        info!("unsubscribe {key:?}");

        let mut map = self.registry.write().await;
        let listeners = map.get_mut(&key);
        if listeners.is_none() {
            return;
        }

        let listeners = listeners.unwrap();

        let index = Self::index_of_listener(listeners, &listener);
        if index.is_none() {
            warn!("listener {key:?} doesn't exist");
            return;
        }

        let index = index.unwrap();
        listeners.remove(index);
    }
}

#[derive(Clone)]
pub(crate) struct ServiceInfoEmitter {
    sender: Sender<(ServiceInfo, Span)>,
    cache: Arc<Cache<ServiceInfo>>,
    push_empty_protection: bool,
}

impl ServiceInfoEmitter {
    fn new(
        sender: Sender<(ServiceInfo, Span)>,
        cache: Arc<Cache<ServiceInfo>>,
        push_empty_protection: bool,
    ) -> Self {
        Self {
            sender,
            cache,
            push_empty_protection,
        }
    }

    #[instrument(skip_all)]
    pub(crate) async fn emit(&self, service_info: ServiceInfo) {
        let notify = self
            .process_service_info(service_info.clone())
            .in_current_span()
            .await;
        let span = Span::current();
        if notify {
            let send_ret = self.sender.send((service_info, span)).await;
            if let Err(e) = send_ret {
                error!("notify observer object failed: {e}");
            }
        }
    }

    fn vec_2_string<T: Serialize>(vec: &Vec<T>) -> String {
        match serde_json::to_string::<Vec<T>>(vec) {
            Ok(json) => json,
            Err(e) => {
                warn!("vec to json string error, it will return default value '[]', {e:?}");
                "[]".to_string()
            }
        }
    }

    fn is_empty_or_error_push(&self, service_info: &ServiceInfo) -> bool {
        service_info.hosts.is_none() || (self.push_empty_protection && !service_info.validate())
    }

    async fn process_service_info(&self, service_info: ServiceInfo) -> bool {
        if self.is_empty_or_error_push(&service_info) {
            warn!("empty or error push: {service_info:?}");
            return false;
        }

        let name =
            ServiceInfo::get_grouped_service_name(&service_info.name, &service_info.group_name);
        let key = ServiceInfo::get_key(&name, &service_info.clusters);

        let changed = {
            let old_service = self.cache.get(&key);
            Self::is_changed_service_info(old_service, &service_info)
        };

        if changed {
            info!(
                "current ips:({}) service: {} -> {}",
                service_info.ip_count(),
                key,
                service_info.hosts_to_json()
            );
        }
        let _ = self.cache.insert(key, service_info);
        changed
    }

    fn is_changed_service_info(
        old_service: Option<CacheRef<'_, ServiceInfo>>,
        new_service: &ServiceInfo,
    ) -> bool {
        let name =
            ServiceInfo::get_grouped_service_name(&new_service.name, &new_service.group_name);
        let key = ServiceInfo::get_key(&name, &new_service.clusters);
        let hosts_json = new_service.hosts_to_json();

        if old_service.is_none() {
            let ip_count = new_service.ip_count();
            info!("init new ips({ip_count}) service: {key} -> {hosts_json}");
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
}

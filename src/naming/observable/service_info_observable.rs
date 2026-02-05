use std::{collections::HashMap, sync::Arc};

use tokio::sync::{
    RwLock,
    mpsc::{Receiver, Sender, channel},
};
use tracing::{Instrument, Span, debug_span, error, info, instrument, warn};

use crate::{
    api::naming::{NamingChangeEvent, NamingEventListener},
    common::{
        cache::{Cache, CacheRef},
        executor,
    },
    naming::dto::ServiceInfo,
};

use super::service_info_diff::{ServiceInfoDiff, is_outdated_data};

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
        vec.iter()
            .position(|subscriber| Arc::ptr_eq(subscriber, target))
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
            let listeners = listeners.expect("Listeners should exist after checking it's not none");
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

        let listeners = listeners.expect("Listeners should exist after checking it's not none");

        let index = Self::index_of_listener(listeners, &listener);
        if index.is_none() {
            warn!("listener {key:?} doesn't exist");
            return;
        }

        let index = index.expect("Listener index should exist after checking it's not none");
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
            let _ = self.cache.insert(key, service_info);
        }
        changed
    }

    fn is_changed_service_info(
        old_service: Option<CacheRef<'_, ServiceInfo>>,
        new_service: &ServiceInfo,
    ) -> bool {
        let key = ServiceInfo::get_key(
            &ServiceInfo::get_grouped_service_name(&new_service.name, &new_service.group_name),
            &new_service.clusters,
        );

        // Handle initial service registration
        let old_service = match old_service {
            None => {
                info!(
                    "init new ips({}) service: {key} -> {}",
                    new_service.ip_count(),
                    new_service.hosts_to_json()
                );
                return true;
            }
            Some(s) => s,
        };

        // Check for outdated data
        if is_outdated_data(&old_service, new_service) {
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
        // Calculate and log differences
        let diff = ServiceInfoDiff::calculate(
            old_service.hosts.as_deref().unwrap_or(&[]),
            new_service.hosts.as_deref().unwrap_or(&[]),
        );

        diff.log_changes(&key);

        diff.changed
    }
}

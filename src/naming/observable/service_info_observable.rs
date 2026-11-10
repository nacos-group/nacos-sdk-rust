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
            // Use key without clusters to match listener registration (aligned with Java client)
            let key =
                ServiceInfo::get_key_without_clusters(&service_info.name, &service_info.group_name);

            let map = registry.read().await;
            let Some(listeners) = map.get(&key) else {
                warn!("the key {key:?} is not subscribed.");
                continue;
            };
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
                // User callbacks are synchronous and may block, so keep them off
                // the SDK runtime.
                let listener_span = span.clone();
                drop(executor::spawn_blocking(move || {
                    let _enter = listener_span.enter();
                    listener.event(naming_event);
                }));
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
        let Some(listeners) = map.get_mut(&key) else {
            return;
        };

        let Some(index) = Self::index_of_listener(listeners, &listener) else {
            warn!("listener {key:?} doesn't exist");
            return;
        };
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
            .process_service_info(&service_info)
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

    async fn process_service_info(&self, service_info: &ServiceInfo) -> bool {
        if self.is_empty_or_error_push(service_info) {
            warn!("empty or error push: {service_info:?}");
            return false;
        }

        // Use key without clusters for cache (aligned with Java client's getKeyWithoutClusters())
        let key =
            ServiceInfo::get_key_without_clusters(&service_info.name, &service_info.group_name);

        let changed = {
            let old_service = self.cache.get(&key);
            Self::is_changed_service_info(old_service, service_info)
        };

        if changed {
            info!(
                "current ips:({}) service: {} -> {}",
                service_info.ip_count(),
                key,
                service_info.hosts_to_json()
            );
            let _ = self.cache.insert(key, service_info.clone());
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

        if new_service.hosts.is_none() && old_service.hosts.is_none() {
            return false;
        }
        if new_service.hosts.is_none() || old_service.hosts.is_none() {
            return true;
        }
        // Calculate and log differences
        let old_hosts = old_service
            .hosts
            .as_deref()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let new_hosts = new_service
            .hosts
            .as_deref()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let diff = ServiceInfoDiff::calculate(old_hosts, new_hosts);

        diff.log_changes(&key);

        diff.changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::naming::ServiceInstance;
    use crate::common::cache::CacheBuilder;

    fn service_info(ips: &[&str]) -> ServiceInfo {
        let hosts = ips
            .iter()
            .map(|ip| ServiceInstance {
                ip: ip.to_string(),
                port: 8848,
                healthy: true,
                weight: 1.0,
                ..Default::default()
            })
            .collect();
        ServiceInfo {
            name: "svc".to_string(),
            group_name: "DEFAULT".to_string(),
            hosts: Some(Arc::new(hosts)),
            ..Default::default()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_blocking_listener_does_not_starve_other_listeners() {
        use crate::api::naming::{NamingChangeEvent, NamingEventListener};
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;
        use std::time::Duration;

        struct BlockingListener {
            entered: Arc<AtomicBool>,
            release: Mutex<mpsc::Receiver<()>>,
        }

        impl NamingEventListener for BlockingListener {
            fn event(&self, _: Arc<NamingChangeEvent>) {
                self.entered.store(true, Ordering::Release);
                let _ = self.release.lock().expect("release receiver").recv();
            }
        }

        struct SignalingListener(Arc<AtomicBool>);

        impl NamingEventListener for SignalingListener {
            fn event(&self, _: Arc<NamingChangeEvent>) {
                self.0.store(true, Ordering::Release);
            }
        }

        let cache = Arc::new(
            CacheBuilder::naming("test-observer".to_string())
                .build()
                .await,
        );
        let (observer, emitter) = create("test-observer".to_string(), cache, false);
        let key = ServiceInfo::get_key_without_clusters("svc", "DEFAULT");
        let entered = Arc::new(AtomicBool::new(false));
        let second_ran = Arc::new(AtomicBool::new(false));
        let (release_tx, release_rx) = mpsc::channel();

        observer
            .subscribe(
                key.clone(),
                Arc::new(BlockingListener {
                    entered: Arc::clone(&entered),
                    release: Mutex::new(release_rx),
                }),
            )
            .await;
        emitter.emit(service_info(&["127.0.0.1"])).await;
        tokio::time::timeout(Duration::from_secs(2), async {
            while !entered.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("blocking listener should start");

        observer
            .subscribe(key, Arc::new(SignalingListener(Arc::clone(&second_ran))))
            .await;
        emitter
            .emit(service_info(&["127.0.0.1", "127.0.0.2"]))
            .await;

        let ran = tokio::time::timeout(Duration::from_secs(2), async {
            while !second_ran.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();

        let _ = release_tx.send(());
        let _ = release_tx.send(());
        assert!(
            ran,
            "a blocking listener must not starve later registered listeners"
        );
    }

    #[tokio::test]
    async fn test_emit_notifies_and_caches_only_on_change() {
        let cache = Arc::new(CacheBuilder::naming("test-ns".to_string()).build().await);
        let (sender, mut receiver) = channel(16);
        let emitter = ServiceInfoEmitter::new(sender, Arc::clone(&cache), false);
        let key = ServiceInfo::get_key_without_clusters("svc", "DEFAULT");

        emitter.emit(service_info(&["127.0.0.1"])).await;
        let (notified, _) = receiver.try_recv().expect("initial push should notify");
        assert_eq!(notified.hosts.as_ref().expect("hosts").len(), 1);
        assert_eq!(
            cache
                .get(&key)
                .expect("cached")
                .hosts
                .as_ref()
                .expect("hosts")
                .len(),
            1
        );

        emitter.emit(service_info(&["127.0.0.1"])).await;
        assert!(
            receiver.try_recv().is_err(),
            "unchanged push must not notify"
        );
        assert!(
            Arc::ptr_eq(
                cache
                    .get(&key)
                    .expect("cached")
                    .hosts
                    .as_ref()
                    .expect("hosts"),
                notified.hosts.as_ref().expect("hosts"),
            ),
            "unchanged push must not replace the cached entry"
        );

        emitter
            .emit(service_info(&["127.0.0.1", "127.0.0.2"]))
            .await;
        let (notified, _) = receiver.try_recv().expect("changed push should notify");
        assert_eq!(notified.hosts.as_ref().expect("hosts").len(), 2);
        assert_eq!(
            cache
                .get(&key)
                .expect("cached")
                .hosts
                .as_ref()
                .expect("hosts")
                .len(),
            2
        );
    }
}

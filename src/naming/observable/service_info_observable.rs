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
use crate::naming::filter_instances_by_clusters;

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

/// A registered listener together with the clusters it asked for.
///
/// The Java client keeps this pairing in `NamingSelectorWrapper`: the server
/// pushes every instance, and each listener only receives the ones matching
/// its own cluster selector.
struct ListenerEntry {
    listener: Arc<dyn NamingEventListener>,
    clusters: Vec<String>,
}

type ListenerRegistry = Arc<RwLock<HashMap<String, Vec<ListenerEntry>>>>;
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
        vec: &[ListenerEntry],
        target: &Arc<dyn NamingEventListener>,
    ) -> Option<usize> {
        vec.iter()
            .position(|entry| Arc::ptr_eq(&entry.listener, target))
    }
}

impl ServiceInfoObserver {
    async fn observe(mut receiver: Receiver<(ServiceInfo, Span)>, registry: ListenerRegistry) {
        info!("service info observe task start!");
        while let Some((service_info, span)) = receiver.recv().await {
            // The cache and the listener registry are keyed by service and
            // group only, never by cluster (aligned with the Java client).
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

            for entry in listeners {
                // The server pushes every instance, so each listener only gets
                // the ones matching the clusters it subscribed to. This is the
                // same narrowing the Java client does with its per-listener
                // cluster selector.
                let instances = if entry.clusters.is_empty() {
                    naming_event.instances.clone()
                } else {
                    naming_event.instances.as_ref().map(|instances| {
                        Arc::new(filter_instances_by_clusters(instances, &entry.clusters))
                    })
                };

                // The event reports the cluster scope of the listener it is
                // delivered to, not the one the server happened to push. The
                // Java client does the same in `NamingSelectorWrapper`, which
                // carries the clusters the listener subscribed with.
                let naming_event = Arc::new(NamingChangeEvent {
                    service_name: naming_event.service_name.clone(),
                    group_name: naming_event.group_name.clone(),
                    clusters: entry.clusters.join(","),
                    instances,
                });

                let listener = entry.listener.clone();
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
    pub(crate) async fn subscribe(
        &self,
        key: String,
        clusters: Vec<String>,
        listener: Arc<dyn NamingEventListener>,
    ) {
        info!("subscribe {key:?}");
        let mut map = self.registry.write().await;
        let listeners = map.get_mut(&key);
        if let Some(listeners) = listeners {
            let index = Self::index_of_listener(listeners, &listener);
            if let Some(index) = index {
                warn!("listener has already exist, remove old listener and then add new listener.");
                listeners.remove(index);
            }
            listeners.push(ListenerEntry { listener, clusters });
        } else {
            let listeners = vec![ListenerEntry { listener, clusters }];
            map.insert(key, listeners);
        }
    }

    /// Removes a listener from the registry.
    ///
    /// Returns whether the service still has listeners left, or `None` when
    /// the listener was not registered at all. The distinction matters:
    /// unsubscribing a listener that was never registered must not drop the
    /// shared server-side subscription, which other listeners still rely on.
    /// The Java client behaves the same way — removing an unknown wrapper is a
    /// no-op, and the subscription is only dropped once the listener set is
    /// empty.
    #[instrument(fields(unsubscribe_key = key), skip_all)]
    pub(crate) async fn unsubscribe(
        &self,
        key: String,
        listener: Arc<dyn NamingEventListener>,
    ) -> Option<bool> {
        info!("unsubscribe {key:?}");

        let mut map = self.registry.write().await;
        let listeners = map.get_mut(&key)?;

        let index = Self::index_of_listener(listeners, &listener)?;
        listeners.remove(index);
        Some(!listeners.is_empty())
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

    fn service_info_with_clusters(ips: &[(&str, &str)]) -> ServiceInfo {
        let hosts = ips
            .iter()
            .map(|(ip, cluster)| ServiceInstance {
                ip: ip.to_string(),
                port: 8848,
                cluster_name: Some(cluster.to_string()),
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
    async fn test_listener_receives_only_its_own_cluster() {
        use crate::api::naming::{NamingChangeEvent, NamingEventListener};
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        struct RecordingListener {
            received: Arc<Mutex<Option<Arc<Vec<ServiceInstance>>>>>,
            clusters: Arc<Mutex<String>>,
            ran: Arc<AtomicBool>,
        }

        impl NamingEventListener for RecordingListener {
            fn event(&self, event: Arc<NamingChangeEvent>) {
                *self.received.lock().expect("received") = event.instances.clone();
                *self.clusters.lock().expect("clusters") = event.clusters.clone();
                self.ran.store(true, Ordering::Release);
            }
        }

        let cache = Arc::new(
            CacheBuilder::naming("test-observer".to_string())
                .build()
                .await,
        );
        let (observer, emitter) = create("test-observer".to_string(), cache, false);
        let key = ServiceInfo::get_key_without_clusters("svc", "DEFAULT");

        let received = Arc::new(Mutex::new(None));
        let clusters = Arc::new(Mutex::new(String::new()));
        let ran = Arc::new(AtomicBool::new(false));
        observer
            .subscribe(
                key,
                vec!["c1".to_string()],
                Arc::new(RecordingListener {
                    received: Arc::clone(&received),
                    clusters: Arc::clone(&clusters),
                    ran: Arc::clone(&ran),
                }),
            )
            .await;

        emitter
            .emit(service_info_with_clusters(&[
                ("10.0.0.1", "c1"),
                ("10.0.0.2", "c2"),
            ]))
            .await;

        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            while !ran.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;

        let instances = received
            .lock()
            .expect("received")
            .clone()
            .expect("instances");
        assert_eq!(
            instances.len(),
            1,
            "a listener must only receive the instances of the cluster it subscribed to"
        );
        assert_eq!(instances[0].ip, "10.0.0.1");

        // The event reports the cluster scope of the listener it is delivered
        // to, the way the Java client's `NamingSelectorWrapper` does — not the
        // cluster the server happened to push.
        assert_eq!(
            clusters.lock().expect("clusters").clone(),
            "c1",
            "the event must report the cluster scope of its own listener"
        );
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
                Vec::new(),
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
            .subscribe(
                key,
                Vec::new(),
                Arc::new(SignalingListener(Arc::clone(&second_ran))),
            )
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

    #[tokio::test]
    async fn test_cache_entry_is_found_by_the_cluster_independent_key() {
        // The cache is written under a cluster-independent key, so a reader
        // using the same key always finds the entry — no matter which cluster
        // the pushed service info happened to carry. This is the regression
        // guard for the cache key mismatch.
        let cache = Arc::new(CacheBuilder::naming("test-ns".to_string()).build().await);
        let (sender, _receiver) = channel(16);
        let emitter = ServiceInfoEmitter::new(sender, Arc::clone(&cache), false);

        let mut pushed = service_info(&["127.0.0.1"]);
        pushed.clusters = "c1".to_string();

        emitter.emit(pushed).await;

        let key = ServiceInfo::get_key_without_clusters("svc", "DEFAULT");
        assert_eq!(key, "DEFAULT@@svc");
        assert!(
            cache.get(&key).is_some(),
            "the cache entry must be reachable through the cluster-independent key"
        );
    }
}

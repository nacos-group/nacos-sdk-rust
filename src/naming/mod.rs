//! Service discovery (naming) implementation for the Nacos SDK.
//!
//! This module provides the internal implementation for service naming/discovery
//! functionality, including:
//!
//! - `chooser`: Load balancing algorithms (random weighted selection)
//! - `dto`: Data transfer objects for service information
//! - `handler`: Server push request handlers for service changes
//! - `message`: gRPC message definitions for naming-related requests/responses
//! - `observable`: Service instance change observation and event notification
//! - `redo`: Automatic retry mechanism for failed operations
//! - `updater`: Background service info updater for subscribed services
//!
//! The public API for service naming is exposed through [`crate::api::naming::NamingService`].

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tracing::{debug, info, instrument, warn};

use crate::api::error::Result;
use crate::api::naming::{NamingEventListener, ServiceInstance};
use crate::api::plugin::AuthPlugin;
use crate::api::props::ClientProps;

use crate::common::cache::{Cache, CacheBuilder};
use crate::common::executor;
use crate::common::remote::grpc::NacosGrpcClient;
use crate::common::remote::grpc::NacosGrpcClientBuilder;
use crate::common::remote::grpc::message::GrpcMessageData;
use crate::common::remote::grpc::message::GrpcRequestMessage;
use crate::common::remote::grpc::message::GrpcResponseMessage;
use crate::common::remote::server_list::create_server_list_provider;
use crate::naming::message::request::*;
use crate::naming::message::response::*;

use self::chooser::RandomWeightChooser;
use self::dto::ServiceInfo;
use self::handler::NamingPushRequestHandler;
use self::observable::service_info_observable::ServiceInfoObserver;
use self::redo::{AutomaticRequest, NamingRedoTask, RedoTask, RedoTaskExecutor};
use self::updater::ServiceInfoUpdater;

mod chooser;
mod dto;
mod handler;
mod message;
mod observable;
mod redo;
mod updater;

pub(crate) struct NacosNamingService {
    nacos_grpc_client: Arc<NacosGrpcClient>,
    namespace: String,
    redo_task_executor: Arc<RedoTaskExecutor>,
    service_info_updater: ServiceInfoUpdater,
    client_id: String,
    naming_cache: Arc<Cache<ServiceInfo>>,
    observer: ServiceInfoObserver,
}

impl std::fmt::Debug for NacosNamingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NacosNamingService")
            .field("namespace", &self.namespace)
            .field("client_id", &self.client_id)
            .finish()
    }
}

const MODULE_NAME: &str = "naming";
static SEQ: AtomicU64 = AtomicU64::new(1);

fn is_selectable(instance: &ServiceInstance, healthy: bool) -> bool {
    healthy == instance.healthy && instance.enabled && instance.weight > 0.0
}

/// Whether `instance` belongs to one of `clusters`; an empty list matches all.
///
/// Like the Java selector, an instance without a cluster name matches no
/// cluster at all: the Java filter compares the raw cluster name, so a missing
/// name never matches a requested cluster.
fn instance_in_clusters(instance: &ServiceInstance, clusters: &[String]) -> bool {
    if clusters.is_empty() {
        return true;
    }
    instance
        .cluster_name
        .as_deref()
        .is_some_and(|cluster_name| clusters.iter().any(|cluster| cluster == cluster_name))
}

/// Whether `instance` satisfies the cluster and health selection.
fn is_selected(instance: &ServiceInstance, clusters: &[String], healthy: bool) -> bool {
    instance_in_clusters(instance, clusters) && is_selectable(instance, healthy)
}

/// Filters instances down to the requested clusters.
///
/// The Java client subscribes with an empty cluster list (`Constants.NULL`) so
/// the server returns every instance, and then narrows the result in memory
/// with a cluster selector. This mirrors that selector: an instance is kept
/// when its cluster name is in `clusters`, and an empty `clusters` keeps all
/// of them.
pub(crate) fn filter_instances_by_clusters(
    instances: &[ServiceInstance],
    clusters: &[String],
) -> Vec<ServiceInstance> {
    if clusters.is_empty() {
        return instances.to_vec();
    }
    instances
        .iter()
        .filter(|instance| instance_in_clusters(instance, clusters))
        .cloned()
        .collect()
}

/// Filters a shared instance list down to the requested clusters.
///
/// When no cluster is requested the original list is returned as-is, so cache
/// hits keep sharing the same allocation.
fn filter_instances_by_clusters_arc(
    instances: Arc<Vec<ServiceInstance>>,
    clusters: &[String],
) -> Arc<Vec<ServiceInstance>> {
    if clusters.is_empty() {
        return instances;
    }
    Arc::new(filter_instances_by_clusters(&instances, clusters))
}

impl NacosNamingService {
    pub(crate) async fn new(
        client_props: ClientProps,
        auth_plugin: Arc<dyn AuthPlugin>,
    ) -> Result<Self> {
        let namespace = client_props.get_namespace_default_if_empty();

        let client_id = crate::common::util::generate_client_id(
            MODULE_NAME,
            &client_props.get_address_identifier(),
            &namespace,
            &SEQ,
        );

        // create redo task executor
        let redo_task_executor = Arc::new(RedoTaskExecutor::new(client_id.clone()));
        let redo_task_executor_on_connected = redo_task_executor.clone();
        let redo_task_executor_on_disconnected = redo_task_executor.clone();

        // create naming cache
        let cache_kind = client_props.get_cache_kind();
        let naming_load_cache_at_start = client_props.get_naming_load_cache_at_start();
        let cache_server_name = client_props.get_cache_server_name();
        let naming_cache: Cache<ServiceInfo> = CacheBuilder::naming(namespace.clone())
            .load_cache_at_start(naming_load_cache_at_start)
            .store(cache_kind, client_props.get_cache_dir(), &cache_server_name)
            .build()
            .await;
        let naming_cache = Arc::new(naming_cache);

        // create naming service info change observable
        let (observer, emitter) = observable::service_info_observable::create(
            client_id.clone(),
            naming_cache.clone(),
            client_props.get_naming_push_empty_protection(),
        );

        let server_request_handler = NamingPushRequestHandler::new(emitter.clone());

        let server_list_provider = create_server_list_provider(&client_props).await?;

        let nacos_grpc_client = NacosGrpcClientBuilder::new(server_list_provider)
            .port(client_props.get_remote_grpc_port())
            .namespace(namespace.clone())
            .client_version(client_props.get_client_version())
            .add_labels(client_props.get_labels())
            .add_default_labels(crate::api::constants::common_remote::LABEL_MODULE_NAMING)
            .app_name(client_props.get_app_name())
            .register_server_request_handler::<NotifySubscriberRequest>(Arc::new(
                server_request_handler,
            ))
            .connected_listener(move |connection_id| {
                info!("connection {} connected.", connection_id);
                let redo = redo_task_executor_on_connected.clone();
                executor::spawn(async move {
                    redo.on_grpc_client_reconnect().await;
                });
            })
            .disconnected_listener(move |connection_id| {
                info!("connection {} disconnected.", connection_id);
                let redo = redo_task_executor_on_disconnected.clone();
                executor::spawn(async move {
                    redo.on_grpc_client_disconnect().await;
                });
            })
            .auth_context(client_props.get_auth_context())
            .auth_plugin(auth_plugin)
            .emergency_start(client_props.get_naming_load_cache_at_start())
            .build(client_id.clone())
            .await?;

        let nacos_grpc_client = Arc::new(nacos_grpc_client);

        // // create service info updater
        let service_info_updater =
            ServiceInfoUpdater::new(emitter, naming_cache.clone(), nacos_grpc_client.clone());

        Ok(NacosNamingService {
            redo_task_executor,
            nacos_grpc_client,
            namespace,
            service_info_updater,
            client_id,
            naming_cache,
            observer,
        })
    }

    async fn request_to_server<R, P>(&self, request: R) -> Result<P>
    where
        R: GrpcRequestMessage + 'static,
        P: GrpcResponseMessage + 'static,
    {
        self.nacos_grpc_client.send_request::<R, P>(request).await
    }

    /// Register a redo task for `request`, returning it so the caller can freeze
    /// it once the operation succeeds.
    async fn register_redo_task<R>(&self, request: &R) -> Arc<NamingRedoTask>
    where
        R: AutomaticRequest + Clone + 'static,
    {
        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = NamingRedoTask::new(self.nacos_grpc_client.clone(), auto_request);
        let redo_task = Arc::new(redo_task);

        // active redo task
        redo_task.active();
        // add redo task to executor
        self.redo_task_executor.add_task(redo_task.clone()).await;
        redo_task
    }

    /// Execute by register request with redo task (active → execute → frozen lifecycle)
    async fn execute_by_register_with_redo<R, P>(&self, request: R, operation: &str) -> Result<()>
    where
        R: GrpcRequestMessage + AutomaticRequest + Clone + 'static,
        P: GrpcResponseMessage + 'static,
    {
        let redo_task = self.register_redo_task(&request).await;

        let response = self.request_to_server::<R, P>(request).await?;
        crate::common::error::handle_response(&response, operation)?;

        redo_task.frozen();
        Ok(())
    }

    /// Execute a deregister request and drop its redo task.
    async fn execute_by_deregister<R, P>(&self, request: R, operation: &str) -> Result<()>
    where
        R: GrpcRequestMessage + AutomaticRequest + 'static,
        P: GrpcResponseMessage + 'static,
    {
        let task_key = request.task_key();

        let response = self.request_to_server::<R, P>(request).await?;
        crate::common::error::handle_response(&response, operation)?;

        self.redo_task_executor.remove_task(&task_key).await;
        Ok(())
    }
}

impl NacosNamingService {
    /// Whether the server-side subscription for this service is still active.
    ///
    /// The redo task registry holds one entry per active subscription, the
    /// same way the Java client checks `isSubscribed`.
    async fn is_subscribed(&self, service_name: &str, group_name: &str) -> bool {
        let task_key = crate::naming::redo::redo_task_key(
            &self.namespace,
            group_name,
            service_name,
            <SubscribeServiceRequest as GrpcMessageData>::identity(),
        );
        self.redo_task_executor.contains_task(&task_key).await
    }

    /// The cached hosts for `key`, gated by the caller's subscription check.
    ///
    /// The outer [`Option`] reports whether a cache entry exists; the inner one
    /// mirrors [`ServiceInfo::hosts`].
    fn cached_hosts(&self, key: &str) -> Option<Option<Arc<Vec<ServiceInstance>>>> {
        self.naming_cache
            .get(key)
            .map(|cached| cached.hosts.clone())
    }

    fn instance_request<R, F>(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
        build: F,
    ) -> R
    where
        F: FnOnce(ServiceInstance, Option<String>, Option<String>, Option<String>) -> R,
    {
        let namespace = Some(self.namespace.clone());
        let group_name = crate::common::util::normalize_group_name(group_name);
        build(
            service_instance,
            Some(service_name),
            namespace,
            Some(group_name),
        )
    }

    async fn register_request<R, F>(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
        build: F,
        operation: &str,
    ) -> Result<()>
    where
        R: GrpcRequestMessage + AutomaticRequest + Clone + 'static,
        F: FnOnce(ServiceInstance, Option<String>, Option<String>, Option<String>) -> R,
    {
        let request = self.instance_request(service_name, group_name, service_instance, build);
        self.execute_by_register_with_redo::<R, InstanceResponse>(request, operation)
            .await
    }

    async fn deregister_request<R, F>(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
        build: F,
        operation: &str,
    ) -> Result<()>
    where
        R: GrpcRequestMessage + AutomaticRequest + Clone + 'static,
        F: FnOnce(ServiceInstance, Option<String>, Option<String>, Option<String>) -> R,
    {
        let request = self.instance_request(service_name, group_name, service_instance, build);
        self.execute_by_deregister::<R, InstanceResponse>(request, operation)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()> {
        let namespace = Some(self.namespace.clone());
        let group_name = crate::common::util::normalize_group_name(group_name);
        let request = BatchInstanceRequest::new(
            service_instances,
            namespace,
            Some(service_name),
            Some(group_name),
        );

        self.execute_by_register_with_redo::<BatchInstanceRequest, BatchInstanceResponse>(
            request,
            "batch_register_instance",
        )
        .await
    }

    /// Fetches the full instance set for a service, before the in-memory
    /// cluster narrowing.
    ///
    /// The selecting APIs apply the cluster and health predicates in one pass
    /// over this list, so the instances are cloned at most once.
    async fn fetch_all_instances(
        &self,
        service_name: String,
        group_name: String,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Arc<Vec<ServiceInstance>>> {
        let hosts = if subscribe {
            // The cache is keyed by service and group only, never by cluster:
            // the Java client stores the full instance set and narrows it down
            // in memory (see `filter_instances_by_clusters`).
            let key = ServiceInfo::get_key_without_clusters(&service_name, &group_name);

            // A cached entry may be left over from a subscription that was
            // dropped, or come from the on-disk cache at startup, so it is
            // only served while the service is still subscribed. This is what
            // the Java client checks in `tryToSubscribe`.
            let cached_hosts = self
                .is_subscribed(&service_name, &group_name)
                .await
                .then(|| self.cached_hosts(&key))
                .flatten();
            match cached_hosts {
                Some(hosts) => {
                    info!("get_all_instances returned cached service_info, group_key={key}");
                    hosts
                }
                _ => self
                    .subscribe_async(service_name, Some(group_name), clusters, None)
                    .await
                    .ok()
                    .flatten(),
            }
        } else {
            let request = ServiceQueryRequest {
                cluster: clusters.join(","),
                group_name: Some(group_name),
                healthy_only: false,
                udp_port: 0,
                namespace: Some(self.namespace.clone()),
                service_name: Some(service_name),
                ..Default::default()
            };

            let response = self
                .request_to_server::<ServiceQueryRequest, QueryServiceResponse>(request)
                .await?;
            crate::common::error::handle_response(&response, "get_all_instances")?;
            response.service_info.hosts
        };
        Ok(hosts.unwrap_or_default())
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn get_all_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Arc<Vec<ServiceInstance>>> {
        let group_name = crate::common::util::normalize_group_name(group_name);

        let hosts = self
            .fetch_all_instances(service_name, group_name, clusters.clone(), subscribe)
            .await?;
        Ok(filter_instances_by_clusters_arc(hosts, &clusters))
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn select_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Arc<Vec<ServiceInstance>>> {
        let group_name = crate::common::util::normalize_group_name(group_name);

        let instances = self
            .fetch_all_instances(service_name, group_name, clusters.clone(), subscribe)
            .await?;
        if instances
            .iter()
            .all(|instance| is_selected(instance, &clusters, healthy))
        {
            return Ok(instances);
        }
        let ret: Vec<ServiceInstance> = instances
            .iter()
            .filter(|instance| is_selected(instance, &clusters, healthy))
            .cloned()
            .collect();
        Ok(Arc::new(ret))
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn select_one_healthy_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Arc<ServiceInstance>> {
        let group_name = crate::common::util::normalize_group_name(group_name);
        let service_name_for_tip = service_name.clone();

        let instances = self
            .fetch_all_instances(service_name, group_name, clusters.clone(), subscribe)
            .await?;
        let chooser = RandomWeightChooser::new(&service_name_for_tip, &instances, |instance| {
            is_selected(instance, &clusters, true)
        })?;
        let index = chooser.choose_index();
        Ok(Arc::new(instances[index].clone()))
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn get_service_list(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)> {
        let group_name = crate::common::util::normalize_group_name(group_name);
        let namespace = Some(self.namespace.clone());

        let request = ServiceListRequest {
            page_no,
            page_size,
            group_name: Some(group_name),
            namespace,
            ..Default::default()
        };

        let response = self
            .request_to_server::<ServiceListRequest, ServiceListResponse>(request)
            .await?;
        crate::common::error::handle_response(&response, "get_service_list")?;

        Ok((response.service_names, response.count))
    }

    async fn subscribe_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Option<Arc<dyn NamingEventListener>>,
    ) -> Result<Option<Arc<Vec<ServiceInstance>>>> {
        let group_name = crate::common::util::normalize_group_name(group_name);
        // The cache is keyed by service and group only, never by cluster:
        // the Java client stores the full instance set and narrows it down
        // in memory (see `filter_instances_by_clusters`).
        let key = ServiceInfo::get_key_without_clusters(&service_name, &group_name);

        // Whether the service is already subscribed. It has to be checked
        // before this call registers its own redo task below.
        //
        // The cache is only served when the service is still subscribed. A
        // cached entry may be left over from a subscription that was dropped,
        // or come from the on-disk cache at startup, and in both cases the
        // client has to talk to the server again. This is what the Java client
        // does in `tryToSubscribe`.
        let is_subscribed = self.is_subscribed(&service_name, &group_name).await;

        // add updater task (for polling refresh when gRPC push is not available)
        self.service_info_updater
            .schedule_update(
                self.namespace.clone(),
                service_name.clone(),
                group_name.clone(),
            )
            .await;

        // add event listener
        if let Some(event_listener) = event_listener {
            self.observer
                .subscribe(key.clone(), clusters.clone(), event_listener)
                .await;
        }

        // Create subscribe request for background execution.
        //
        // The cluster list is deliberately left empty: the Java client always
        // subscribes with `Constants.NULL` so the server returns every
        // instance and the cache holds the complete set. Requesting a subset
        // would make the cached instances depend on which cluster happened to
        // be subscribed first.
        let request = SubscribeServiceRequest::new(
            true,
            String::new(),
            Some(service_name),
            Some(self.namespace.clone()),
            Some(group_name),
        );

        // The redo task retries the subscribe in the background while it stays
        // active, and is frozen once the server acknowledges it.
        let redo_task = self.register_redo_task(&request).await;

        // Try to get service info from cache first (if load_cache_at_start is enabled).
        //
        // The cache is only served when the service is still subscribed, so a
        // stale entry left behind by a dropped subscription is never returned.
        if is_subscribed && let Some(hosts) = self.cached_hosts(&key) {
            info!("subscribe returned cached service_info, group_key={key}");

            // Spawn the actual server request in the background to avoid blocking.
            // The redo task will automatically retry if the server is unavailable.
            // The initial response data will come from cache (if available) or be empty.
            let remote_client = self.nacos_grpc_client.clone();
            executor::spawn(async move {
                let response = remote_client
                    .send_request::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
                    .await;
                if let Ok(resp) = response
                    && crate::common::error::handle_response(&resp, "subscribe").is_ok()
                {
                    debug!("subscribe the {resp:?}");
                    redo_task.frozen();
                }
            });

            return Ok(hosts);
        }

        // Subscribe to the server. When the server cannot be reached, fall
        // back to the cached service info instead of reporting an empty
        // result — this is what the Java client does in `tryToSubscribe`,
        // and it is the whole point of `naming_load_cache_at_start`: serving
        // the last known instances while the server is down.
        let response = match self
            .request_to_server::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
            .await
        {
            Ok(response) => {
                crate::common::error::handle_response(&response, "subscribe")?;
                debug!("subscribe the {response:?}");
                redo_task.frozen();
                response.service_info.hosts
            }
            Err(e) => {
                warn!("subscribe failed, will use the local cache. fail message: {e}");
                match self.cached_hosts(&key) {
                    Some(hosts) => hosts,
                    None => return Err(e),
                }
            }
        };

        Ok(response)
    }
    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn unsubscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        // The listener is matched by identity, so the clusters it subscribed
        // with are not needed here any more. The parameter is kept because it
        // is part of the public API.
        _clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        let group_name = crate::common::util::normalize_group_name(group_name);

        // The listener registry is keyed by service and group only, never by
        // cluster (aligned with the Java client).
        let key = ServiceInfo::get_key_without_clusters(&service_name, &group_name);
        let remaining_listeners = self.observer.unsubscribe(key, event_listener).await;

        // The server-side subscription and the polling task are shared by all
        // the listeners of a service, so they are only dropped once the last
        // listener is gone. This mirrors the Java client, which checks
        // `changeNotifier.isSubscribed` before unsubscribing.
        //
        // A listener that was not registered is a no-op: unsubscribing it must
        // not drop the subscription the other listeners still rely on.
        if remaining_listeners != Some(false) {
            return Ok(());
        }

        // stop updater task
        self.service_info_updater
            .stop_update(service_name.clone(), group_name.clone())
            .await;

        // The cluster list is deliberately left empty, matching the subscribe
        // request it has to cancel out.
        let request = SubscribeServiceRequest::new(
            false,
            String::new(),
            Some(service_name),
            Some(self.namespace.clone()),
            Some(group_name),
        );

        let task_key = request.task_key();

        let response = self
            .request_to_server::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
            .await?;
        crate::common::error::handle_response(&response, "unsubscribe")?;
        debug!("unsubscribe the {response:?}");
        self.redo_task_executor.remove_task(&task_key).await;
        Ok(())
    }
}

impl NacosNamingService {
    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        if service_instance.ephemeral {
            info!(
                "register ephemeral instance: service_name: {service_name}, group_name: {group_name:?}"
            );
            self.register_request(
                service_name,
                group_name,
                service_instance,
                InstanceRequest::register,
                "register_ephemeral_instance",
            )
            .await
        } else {
            info!(
                "register persistent instance: service_name: {service_name}, group_name: {group_name:?}"
            );
            self.register_request(
                service_name,
                group_name,
                service_instance,
                PersistentInstanceRequest::register,
                "register_persistent_instance",
            )
            .await
        }
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        if service_instance.ephemeral {
            info!(
                "deregister ephemeral instance: service_name: {service_name}, group_name: {group_name:?}"
            );
            self.deregister_request(
                service_name,
                group_name,
                service_instance,
                InstanceRequest::deregister,
                "deregister_ephemeral_instance",
            )
            .await
        } else {
            info!(
                "deregister persistent instance: service_name: {service_name}, group_name: {group_name:?}"
            );
            self.deregister_request(
                service_name,
                group_name,
                service_instance,
                PersistentInstanceRequest::deregister,
                "deregister_persistent_instance",
            )
            .await
        }
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    pub(crate) async fn subscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        let _ = self
            .subscribe_async(service_name, group_name, clusters, Some(event_listener))
            .await;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;

use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tracing::{debug, info, instrument};

use crate::api::error::Error::ErrResult;
use crate::api::error::Result;
use crate::api::naming::{InstanceChooser, NamingEventListener, NamingService, ServiceInstance};
use crate::api::plugin::AuthPlugin;
use crate::api::props::ClientProps;

use crate::common::cache::{Cache, CacheBuilder};
use crate::common::executor;
use crate::common::remote::grpc::layers::auth::AuthLayer;
use crate::common::remote::grpc::message::GrpcRequestMessage;
use crate::common::remote::grpc::message::GrpcResponseMessage;
use crate::common::remote::grpc::NacosGrpcClient;
use crate::common::remote::grpc::NacosGrpcClientBuilder;
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

const MODULE_NAME: &str = "naming";
static SEQ: AtomicU64 = AtomicU64::new(1);

fn generate_client_id(server_addr: &str, namespace: &str) -> String {
    let client_id = format!(
        "{MODULE_NAME}:{server_addr}:{namespace}:{}",
        SEQ.fetch_add(1, Ordering::SeqCst)
    );
    client_id
}

impl NacosNamingService {
    pub(crate) fn new(client_props: ClientProps, auth_plugin: Arc<dyn AuthPlugin>) -> Result<Self> {
        let server_list = Arc::new(client_props.get_server_list()?);

        let mut namespace = client_props.get_namespace();
        if namespace.is_empty() {
            namespace = crate::api::constants::DEFAULT_NAMESPACE.to_owned();
        }

        // create client id
        let client_id = generate_client_id(&client_props.get_server_addr(), &namespace);

        // create redo task executor
        let redo_task_executor = Arc::new(RedoTaskExecutor::new(client_id.clone()));
        let redo_task_executor_on_connected = redo_task_executor.clone();
        let redo_task_executor_on_disconnected = redo_task_executor.clone();

        // create naming cache
        let naming_cache: Cache<ServiceInfo> = CacheBuilder::naming(namespace.clone())
            .disk_store()
            .build(client_id.clone());
        let naming_cache = Arc::new(naming_cache);

        // create naming service info change observable
        let (observer, emitter) = observable::service_info_observable::create(
            client_id.clone(),
            naming_cache.clone(),
            client_props.get_naming_push_empty_protection(),
        );

        let server_request_handler = NamingPushRequestHandler::new(emitter.clone());

        let auth_layer = Arc::new(AuthLayer::new(
            auth_plugin,
            server_list.to_vec(),
            client_props.get_auth_context(),
            client_id.clone(),
        ));

        let nacos_grpc_client = NacosGrpcClientBuilder::new(server_list.to_vec())
            .port(client_props.get_remote_grpc_port())
            .namespace(namespace.clone())
            .client_version(client_props.get_client_version())
            .support_remote_connection(true)
            .support_config_remote_metrics(true)
            .support_naming_delta_push(false)
            .support_naming_remote_metric(false)
            .add_labels(client_props.get_labels())
            .add_label(
                crate::api::constants::common_remote::LABEL_SOURCE.to_owned(),
                crate::api::constants::common_remote::LABEL_SOURCE_SDK.to_owned(),
            )
            .add_label(
                crate::api::constants::common_remote::LABEL_MODULE.to_owned(),
                crate::api::constants::common_remote::LABEL_MODULE_NAMING.to_owned(),
            )
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
            .unary_call_layer(auth_layer.clone())
            .bi_call_layer(auth_layer)
            .build(client_id.clone());

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
}

impl NacosNamingService {
    async fn register_ephemeral_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        info!(
            "register ephemeral instance: service_name: {service_name}, group_name: {group_name:?}"
        );
        let namespace = Some(self.namespace.clone());
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
        let request = InstanceRequest::register(
            service_instance,
            Some(service_name),
            namespace,
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = Arc::new(NamingRedoTask::new(
            self.nacos_grpc_client.clone(),
            auto_request,
        ));

        // active redo task
        redo_task.active();
        // add redo task to executor
        self.redo_task_executor.add_task(redo_task.clone()).await;

        let body = self
            .request_to_server::<InstanceRequest, InstanceResponse>(request)
            .await?;
        if !body.is_success() {
            return Err(ErrResult(format!(
                "naming service register ephemeral service failed: resultCode: {}, errorCode:{}, message:{}",
                body.result_code,
                body.error_code,
                body.message.unwrap_or_default()
            )));
        }

        redo_task.frozen();
        Ok(())
    }

    async fn register_persistent_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        info!("register persistent instance: service_name: {service_name}, group_name: {group_name:?}");
        let namespace = Some(self.namespace.clone());
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
        let request = PersistentInstanceRequest::register(
            service_instance,
            Some(service_name),
            namespace,
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = Arc::new(NamingRedoTask::new(
            self.nacos_grpc_client.clone(),
            auto_request,
        ));

        // active redo task
        redo_task.active();
        // add redo task to executor
        self.redo_task_executor.add_task(redo_task.clone()).await;

        let body = self
            .request_to_server::<PersistentInstanceRequest, InstanceResponse>(request)
            .await?;
        if !body.is_success() {
            return Err(ErrResult(format!(
                    "naming service register persistent service failed: resultCode: {}, errorCode:{}, message:{}",
                    body.result_code,
                    body.error_code,
                    body.message.unwrap_or_default()
                )));
        }

        redo_task.frozen();
        Ok(())
    }

    async fn deregister_ephemeral_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        info!("deregister ephemeral instance: service_name: {service_name}, group_name: {group_name:?}");

        let namespace = Some(self.namespace.clone());
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
        let request = InstanceRequest::deregister(
            service_instance,
            Some(service_name),
            namespace,
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = NamingRedoTask::new(self.nacos_grpc_client.clone(), auto_request);

        let body = self
            .request_to_server::<InstanceRequest, InstanceResponse>(request)
            .await?;

        if !body.is_success() {
            return Err(ErrResult(format!("naming service deregister ephemeral service failed: resultCode: {}, errorCode:{}, message:{}", body.result_code,  body.error_code, body.message.unwrap_or_default())));
        }

        // remove redo task from executor
        self.redo_task_executor
            .remove_task(redo_task.task_key().as_str())
            .await;
        Ok(())
    }

    async fn deregister_persistent_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        info!("deregister persistent instance: service_name: {service_name}, group_name: {group_name:?}");
        let namespace = Some(self.namespace.clone());
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
        let request = PersistentInstanceRequest::deregister(
            service_instance,
            Some(service_name),
            namespace,
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = NamingRedoTask::new(self.nacos_grpc_client.clone(), auto_request);

        let body = self
            .request_to_server::<PersistentInstanceRequest, InstanceResponse>(request)
            .await?;

        if !body.is_success() {
            return Err(ErrResult(format!("naming service deregister persistent service failed: resultCode: {}, errorCode:{}, message:{}", body.result_code,  body.error_code, body.message.unwrap_or_default())));
        }

        // remove redo task from executor
        self.redo_task_executor
            .remove_task(redo_task.task_key().as_str())
            .await;
        Ok(())
    }

    async fn batch_register_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()> {
        let namespace = Some(self.namespace.clone());
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
        let request = BatchInstanceRequest::new(
            service_instances,
            namespace,
            Some(service_name),
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = NamingRedoTask::new(self.nacos_grpc_client.clone(), auto_request);
        let redo_task = Arc::new(redo_task);

        // active redo task
        redo_task.active();
        // add redo task to executor
        self.redo_task_executor.add_task(redo_task.clone()).await;

        let body = self
            .request_to_server::<BatchInstanceRequest, BatchInstanceResponse>(request)
            .await?;
        if !body.is_success() {
            return Err(ErrResult(format!("naming service batch register services failed: resultCode: {}, errorCode:{}, message:{}", body.result_code,  body.error_code, body.message.unwrap_or_default())));
        }
        redo_task.frozen();
        Ok(())
    }

    async fn get_all_instances_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Vec<ServiceInstance>> {
        let cluster_str = clusters.join(",");
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());

        let service_info;
        if subscribe {
            let cache_service_info = {
                let grouped_name =
                    ServiceInfo::get_grouped_service_name(&service_name, &group_name);
                let key = ServiceInfo::get_key(&grouped_name, &cluster_str);
                let ret = self.naming_cache.get(&key).map(|data| data.clone());
                ret
            };

            if cache_service_info.is_none() {
                let subscribe_service_info = self
                    .subscribe_async(service_name, Some(group_name), clusters, None)
                    .await;
                if let Ok(subscribe_service_info) = subscribe_service_info {
                    service_info = Some(subscribe_service_info);
                } else {
                    service_info = None;
                }
            } else {
                service_info = Some(cache_service_info.unwrap());
            }
        } else {
            let request = ServiceQueryRequest {
                cluster: cluster_str,
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
            if !response.is_success() {
                return Err(ErrResult(format!("naming service query services failed: resultCode: {}, errorCode:{}, message:{}", response.result_code,  response.error_code, response.message.unwrap_or_default())));
            }
            service_info = Some(response.service_info);
        }
        if service_info.is_none() {
            return Ok(Vec::default());
        }
        let service_info = service_info.unwrap();
        let instances = service_info.hosts;
        if instances.is_none() {
            return Ok(Vec::default());
        }
        Ok(instances.unwrap())
    }

    async fn select_instances_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Vec<ServiceInstance>> {
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());

        let all_instance = self
            .get_all_instances_async(service_name, Some(group_name), clusters, subscribe)
            .await?;
        let ret: Vec<ServiceInstance> = all_instance
            .into_iter()
            .filter(|instance| {
                healthy == instance.healthy && instance.enabled && instance.weight > 0.0
            })
            .collect();
        Ok(ret)
    }

    async fn select_one_healthy_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<ServiceInstance> {
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
        let service_name_for_tip = service_name.clone();

        let ret = self
            .select_instances_async(
                service_name.clone(),
                Some(group_name),
                clusters,
                subscribe,
                true,
            )
            .await?;
        let chooser = RandomWeightChooser::new(service_name, ret)?;
        let instance = chooser.choose();
        if instance.is_none() {
            return Err(ErrResult(format!(
                "no available {service_name_for_tip} service instance can be selected"
            )));
        }
        let instance = instance.unwrap();
        Ok(instance)
    }

    async fn get_service_list_async(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)> {
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());
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
        if !response.is_success() {
            return Err(ErrResult(format!(
                "naming service list services failed: resultCode: {}, errorCode:{}, message:{}",
                response.result_code,
                response.error_code,
                response.message.unwrap_or_default()
            )));
        }

        Ok((response.service_names, response.count))
    }

    async fn subscribe_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Option<Arc<dyn NamingEventListener>>,
    ) -> Result<ServiceInfo> {
        let clusters = clusters.join(",");
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());

        // add updater task
        self.service_info_updater
            .schedule_update(
                self.namespace.clone(),
                service_name.clone(),
                group_name.clone(),
                clusters.clone(),
            )
            .await;

        // add event listener
        if let Some(event_listener) = event_listener {
            let grouped_name = ServiceInfo::get_grouped_service_name(&service_name, &group_name);
            let key = ServiceInfo::get_key(&grouped_name, &clusters);
            self.observer.subscribe(key, event_listener).await;
        }

        let request = SubscribeServiceRequest::new(
            true,
            clusters,
            Some(service_name),
            Some(self.namespace.clone()),
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = NamingRedoTask::new(self.nacos_grpc_client.clone(), auto_request);

        let redo_task = Arc::new(redo_task);
        // active redo task
        redo_task.active();
        // add redo task to executor
        self.redo_task_executor.add_task(redo_task.clone()).await;

        let response = self
            .request_to_server::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
            .await?;
        if !response.is_success() {
            return Err(ErrResult(format!(
                "naming subscribe services failed: resultCode: {}, errorCode:{}, message:{}",
                response.result_code,
                response.error_code,
                response.message.unwrap_or_default()
            )));
        }

        debug!("subscribe the {response:?}");
        redo_task.frozen();

        Ok(response.service_info)
    }

    async fn unsubscribe_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Option<Arc<dyn NamingEventListener>>,
    ) -> Result<()> {
        let clusters = clusters.join(",");
        let group_name = group_name
            .filter(|data| !data.is_empty())
            .unwrap_or_else(|| crate::api::constants::DEFAULT_GROUP.to_owned());

        // stop updater task
        self.service_info_updater
            .stop_update(service_name.clone(), group_name.clone(), clusters.clone())
            .await;

        // remove event listener
        if let Some(event_listener) = event_listener {
            let grouped_name = ServiceInfo::get_grouped_service_name(&service_name, &group_name);
            let key = ServiceInfo::get_key(&grouped_name, &clusters);

            self.observer.unsubscribe(key, event_listener).await;
        }

        let request = SubscribeServiceRequest::new(
            false,
            clusters,
            Some(service_name),
            Some(self.namespace.clone()),
            Some(group_name),
        );

        // automatic request
        let auto_request: Arc<dyn AutomaticRequest> = Arc::new(request.clone());
        let redo_task = NamingRedoTask::new(self.nacos_grpc_client.clone(), auto_request);

        let response = self
            .request_to_server::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
            .await?;
        if !response.is_success() {
            return Err(ErrResult(format!(
                "naming subscribe services failed: resultCode: {}, errorCode:{}, message:{}",
                response.result_code,
                response.error_code,
                response.message.unwrap_or_default()
            )));
        }
        debug!("unsubscribe the {response:?}");
        self.redo_task_executor
            .remove_task(redo_task.task_key().as_str())
            .await;
        Ok(())
    }
}

#[cfg(not(feature = "async"))]
impl NamingService for NacosNamingService {
    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        if service_instance.ephemeral {
            let future = self.deregister_ephemeral_instance_async(
                service_name,
                group_name,
                service_instance,
            );
            futures::executor::block_on(future)
        } else {
            let future = self.deregister_persistent_instance_async(
                service_name,
                group_name,
                service_instance,
            );
            futures::executor::block_on(future)
        }
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()> {
        let future =
            self.batch_register_instance_async(service_name, group_name, service_instances);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn get_all_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Vec<ServiceInstance>> {
        let future = self.get_all_instances_async(service_name, group_name, clusters, subscribe);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn select_one_healthy_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<ServiceInstance> {
        let future =
            self.select_one_healthy_instance_async(service_name, group_name, clusters, subscribe);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn get_service_list(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)> {
        let future = self.get_service_list_async(page_no, page_size, group_name);
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn subscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        let future = self.subscribe_async(service_name, group_name, clusters, Some(event_listener));
        let _ = futures::executor::block_on(future);
        Ok(())
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn unsubscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        let future =
            self.unsubscribe_async(service_name, group_name, clusters, Some(event_listener));
        futures::executor::block_on(future)
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        if service_instance.ephemeral {
            let future =
                self.register_ephemeral_instance_async(service_name, group_name, service_instance);
            futures::executor::block_on(future)
        } else {
            let future =
                self.register_persistent_instance_async(service_name, group_name, service_instance);
            futures::executor::block_on(future)
        }
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    fn select_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Vec<ServiceInstance>> {
        let future =
            self.select_instances_async(service_name, group_name, clusters, subscribe, healthy);
        futures::executor::block_on(future)
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl NamingService for NacosNamingService {
    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        if service_instance.ephemeral {
            self.deregister_ephemeral_instance_async(service_name, group_name, service_instance)
                .await
        } else {
            self.deregister_persistent_instance_async(service_name, group_name, service_instance)
                .await
        }
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()> {
        self.batch_register_instance_async(service_name, group_name, service_instances)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn get_all_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Vec<ServiceInstance>> {
        self.get_all_instances_async(service_name, group_name, clusters, subscribe)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn select_one_healthy_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<ServiceInstance> {
        self.select_one_healthy_instance_async(service_name, group_name, clusters, subscribe)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn get_service_list(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)> {
        self.get_service_list_async(page_no, page_size, group_name)
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn subscribe(
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

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn unsubscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        self.unsubscribe_async(service_name, group_name, clusters, Some(event_listener))
            .await
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        if service_instance.ephemeral {
            self.register_ephemeral_instance_async(service_name, group_name, service_instance)
                .await
        } else {
            self.register_persistent_instance_async(service_name, group_name, service_instance)
                .await
        }
    }

    #[instrument(fields(client_id = &self.client_id, group = group_name), skip_all)]
    async fn select_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Vec<ServiceInstance>> {
        self.select_instances_async(service_name, group_name, clusters, subscribe, healthy)
            .await
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use core::time;
    use std::{collections::HashMap, thread};

    use tracing::{info, metadata::LevelFilter};

    use crate::api::{naming::NamingChangeEvent, plugin::NoopAuthPlugin};

    use super::*;

    #[test]
    #[ignore]
    fn test_ephemeral_register_service() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata,
            ..Default::default()
        };

        let ret = naming_service.register_instance(
            "test-ephemeral-service".to_string(),
            None,
            service_instance,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(100);
        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_persistent_register_service() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 8848,
            ephemeral: false,
            metadata,
            ..Default::default()
        };

        let ret = naming_service.register_instance(
            "test-persistent-service".to_string(),
            None,
            service_instance,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(100);
        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_register_and_deregister_persistent_service() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 8848,
            metadata,
            ephemeral: false,
            ..Default::default()
        };

        let ret = naming_service.register_instance(
            "test-persistent-service".to_string(),
            None,
            service_instance.clone(),
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(30);
        thread::sleep(ten_millis);

        let ret = naming_service.deregister_instance(
            "test-persistent-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            service_instance,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(30);
        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_register_and_deregister_ephemeral_service() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata,
            ..Default::default()
        };

        let ret = naming_service.register_instance(
            "test-ephemeral-service".to_string(),
            None,
            service_instance.clone(),
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(30);
        thread::sleep(ten_millis);

        let ret = naming_service.deregister_instance(
            "test-ephemeral-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            service_instance,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(30);
        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_batch_register_service() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance1 = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance2 = ServiceInstance {
            ip: "192.168.1.1".to_string(),
            port: 8888,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance3 = ServiceInstance {
            ip: "172.0.2.1".to_string(),
            port: 6666,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let ret = naming_service.batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(300);
        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_batch_register_service_and_query_all_instances() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance1 = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance2 = ServiceInstance {
            ip: "192.168.1.1".to_string(),
            port: 8888,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance3 = ServiceInstance {
            ip: "172.0.2.1".to_string(),
            port: 6666,
            metadata: metadata.clone(),
            ..Default::default()
        };
        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let ret = naming_service.batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(10);
        thread::sleep(ten_millis);

        let all_instances = naming_service.get_all_instances(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            false,
        );
        info!("response. {all_instances:?}");

        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_select_instance() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance1 = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance2 = ServiceInstance {
            ip: "192.168.1.1".to_string(),
            port: 8888,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance3 = ServiceInstance {
            ip: "172.0.2.1".to_string(),
            port: 6666,
            metadata: metadata.clone(),
            ..Default::default()
        };
        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let ret = naming_service.batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(10);
        thread::sleep(ten_millis);

        let all_instances = naming_service.select_instances(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            false,
            true,
        );
        info!("response. {all_instances:?}");

        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_select_one_healthy_instance() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance1 = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance2 = ServiceInstance {
            ip: "192.168.1.1".to_string(),
            port: 8888,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance3 = ServiceInstance {
            ip: "172.0.2.1".to_string(),
            port: 6666,
            metadata: metadata.clone(),
            ..Default::default()
        };
        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let ret = naming_service.batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(10);
        thread::sleep(ten_millis);

        for _ in 0..3 {
            let all_instances = naming_service.select_one_healthy_instance(
                "test-service".to_string(),
                Some(crate::api::constants::DEFAULT_GROUP.to_string()),
                Vec::default(),
                false,
            );
            info!("response. {all_instances:?}");
        }

        thread::sleep(ten_millis);
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_get_service_list() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance1 = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance2 = ServiceInstance {
            ip: "192.168.1.1".to_string(),
            port: 8888,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance3 = ServiceInstance {
            ip: "172.0.2.1".to_string(),
            port: 6666,
            metadata: metadata.clone(),
            ..Default::default()
        };
        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let ret = naming_service.batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        );
        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(10);
        thread::sleep(ten_millis);

        let service_list = naming_service.get_service_list(1, 50, None);
        info!("response. {service_list:?}");

        thread::sleep(ten_millis);
        Ok(())
    }

    #[derive(Hash, PartialEq)]
    pub struct InstancesChangeEventListener;

    impl NamingEventListener for InstancesChangeEventListener {
        fn event(&self, event: Arc<NamingChangeEvent>) {
            info!("InstancesChangeEventListener: {event:?}");
        }
    }

    #[test]
    #[ignore]
    fn test_service_push() -> Result<()> {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let props = ClientProps::new().server_addr("127.0.0.1:8848");

        let mut metadata = HashMap::<String, String>::new();
        metadata.insert("netType".to_string(), "external".to_string());
        metadata.insert("version".to_string(), "2.0".to_string());

        let naming_service = NacosNamingService::new(props, Arc::new(NoopAuthPlugin::default()))?;
        let service_instance1 = ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 9090,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance2 = ServiceInstance {
            ip: "192.168.1.1".to_string(),
            port: 8888,
            metadata: metadata.clone(),
            ..Default::default()
        };

        let service_instance3 = ServiceInstance {
            ip: "172.0.2.1".to_string(),
            port: 6666,
            metadata: metadata.clone(),
            ..Default::default()
        };
        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let ret = naming_service.batch_register_instance(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            instance_vec,
        );
        info!("response. {ret:?}");

        let listener = Arc::new(InstancesChangeEventListener);
        let ret = naming_service.subscribe(
            "test-service".to_string(),
            Some(crate::api::constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            listener,
        );

        info!("response. {ret:?}");

        let ten_millis = time::Duration::from_secs(3000);
        thread::sleep(ten_millis);
        Ok(())
    }
}

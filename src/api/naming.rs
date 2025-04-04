use std::fmt::Debug;
use std::{collections::HashMap, sync::Arc};

use crate::api::plugin;
use crate::{api::error::Result, naming::NacosNamingService};
use serde::{Deserialize, Serialize};

use super::props::ClientProps;

const DEFAULT_CLUSTER_NAME: &str = "DEFAULT";

/// ServiceInstance for api.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInstance {
    pub instance_id: Option<String>,

    pub ip: String,

    pub port: i32,

    pub weight: f64,

    pub healthy: bool,

    pub enabled: bool,

    pub ephemeral: bool,

    pub cluster_name: Option<String>,

    pub service_name: Option<String>,

    pub metadata: HashMap<String, String>,
}

impl ServiceInstance {
    pub fn instance_id(&self) -> Option<&String> {
        self.instance_id.as_ref()
    }

    pub fn ip(&self) -> &str {
        &self.ip
    }

    pub fn port(&self) -> i32 {
        self.port
    }

    pub fn weight(&self) -> f64 {
        self.weight
    }

    pub fn healthy(&self) -> bool {
        self.healthy
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn ephemeral(&self) -> bool {
        self.ephemeral
    }

    pub fn cluster_name(&self) -> Option<&String> {
        self.cluster_name.as_ref()
    }

    pub fn service_name(&self) -> Option<&String> {
        self.service_name.as_ref()
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    pub fn ip_and_port(&self) -> String {
        format!("{}:{}", &self.ip, self.port)
    }

    pub fn is_same_instance(&self, other: &ServiceInstance) -> bool {
        self.instance_id == other.instance_id
            && self.ip == other.ip
            && self.port == other.port
            && self.weight == other.weight
            && self.healthy == other.healthy
            && self.enabled == other.enabled
            && self.ephemeral == other.ephemeral
            && self.cluster_name == other.cluster_name
            && self.service_name == other.service_name
            && self.metadata == other.metadata
    }
}

impl Default for ServiceInstance {
    fn default() -> Self {
        Self {
            instance_id: Default::default(),
            ip: Default::default(),
            port: Default::default(),
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some(DEFAULT_CLUSTER_NAME.to_owned()),
            service_name: Default::default(),
            metadata: Default::default(),
        }
    }
}

/// NamingChangeEvent when Instance change.
#[derive(Clone, Debug)]
pub struct NamingChangeEvent {
    pub service_name: String,
    pub group_name: String,
    pub clusters: String,
    pub instances: Option<Vec<ServiceInstance>>,
}

pub trait InstanceChooser {
    fn choose(self) -> Option<ServiceInstance>;
}

/// The NamingEventListener receive an event of [`NamingChangeEvent`].
pub trait NamingEventListener: Send + Sync + 'static {
    fn event(&self, event: Arc<NamingChangeEvent>);
}

/// Api [`NamingService`].
///
/// # Examples
///
/// ```ignore
///  let mut naming_service = nacos_sdk::api::naming::NamingServiceBuilder::new(
///        nacos_sdk::api::props::ClientProps::new()
///           .server_addr("127.0.0.1:8848")
///           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///           .namespace("")
///           .app_name("todo-your-app-name"),
///   )
///   .build()?;
/// ```
#[doc(alias("naming", "sdk", "api"))]
#[derive(Clone, Debug)]
pub struct NamingService {
    inner: Arc<NacosNamingService>,
}

impl NamingService {
    pub async fn register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .register_instance(service_name, group_name, service_instance)
            .await
    }

    pub async fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .deregister_instance(service_name, group_name, service_instance)
            .await
    }

    pub async fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .batch_register_instance(service_name, group_name, service_instances)
            .await
    }

    pub async fn get_all_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Vec<ServiceInstance>> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .get_all_instances(service_name, group_name, clusters, subscribe)
            .await
    }

    pub async fn select_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Vec<ServiceInstance>> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .select_instances(service_name, group_name, clusters, subscribe, healthy)
            .await
    }

    pub async fn select_one_healthy_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<ServiceInstance> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .select_one_healthy_instance(service_name, group_name, clusters, subscribe)
            .await
    }

    pub async fn get_service_list(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)> {
        self.inner
            .get_service_list(page_no, page_size, group_name)
            .await
    }

    pub async fn subscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .subscribe(service_name, group_name, clusters, event_listener)
            .await
    }

    pub async fn unsubscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()> {
        crate::common::util::check_not_blank(&service_name, "service_name")?;
        self.inner
            .unsubscribe(service_name, group_name, clusters, event_listener)
            .await
    }
}

/// Builder of api [`NamingService`].
///
/// # Examples
///
/// ```ignore
///  let mut naming_service = nacos_sdk::api::naming::NamingServiceBuilder::new(
///        nacos_sdk::api::props::ClientProps::new()
///           .server_addr("127.0.0.1:8848")
///           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///           .namespace("")
///           .app_name("todo-your-app-name"),
///   )
///   .build()?;
/// ```
#[doc(alias("naming", "builder"))]
pub struct NamingServiceBuilder {
    client_props: ClientProps,
    auth_plugin: Option<Arc<dyn plugin::AuthPlugin>>,
}

impl NamingServiceBuilder {
    pub fn new(client_props: ClientProps) -> Self {
        NamingServiceBuilder {
            client_props,
            auth_plugin: None,
        }
    }

    #[cfg(feature = "auth-by-http")]
    pub fn enable_auth_plugin_http(self) -> Self {
        self.with_auth_plugin(Arc::new(plugin::HttpLoginAuthPlugin::default()))
    }

    #[cfg(feature = "auth-by-aliyun")]
    pub fn enable_auth_plugin_aliyun(self) -> Self {
        self.with_auth_plugin(Arc::new(plugin::AliyunRamAuthPlugin::default()))
    }

    /// Set [`plugin::AuthPlugin`]
    pub fn with_auth_plugin(mut self, auth_plugin: Arc<dyn plugin::AuthPlugin>) -> Self {
        self.auth_plugin = Some(auth_plugin);
        self
    }

    pub fn build(self) -> Result<NamingService> {
        let auth_plugin = match self.auth_plugin {
            None => Arc::new(plugin::NoopAuthPlugin::default()),
            Some(plugin) => plugin,
        };
        let inner = NacosNamingService::new(self.client_props, auth_plugin)?;
        let inner = Arc::new(inner);
        Ok(NamingService { inner })
    }
}

impl Default for NamingServiceBuilder {
    fn default() -> Self {
        NamingServiceBuilder {
            client_props: ClientProps::new(),
            auth_plugin: None,
        }
    }
}

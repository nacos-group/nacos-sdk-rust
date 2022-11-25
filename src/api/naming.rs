use std::time::SystemTime;
use std::{collections::HashMap, sync::Arc};

use crate::api::plugin;
use crate::{api::error::Result, naming::NacosNamingService};
use serde::{Deserialize, Serialize};
use tracing::error;

use super::props::ClientProps;
use crate::api::error::Error::GroupNameParseErr;

const DEFAULT_CLUSTER_NAME: &str = "DEFAULT";

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,

    pub group_name: String,

    pub clusters: String,

    pub cache_millis: i64,

    pub last_ref_time: i64,

    pub checksum: String,

    #[serde(rename = "allIPs")]
    pub all_ips: bool,

    pub reach_protection_threshold: bool,

    pub hosts: Option<Vec<ServiceInstance>>,
}

const SERVICE_INFO_SEPARATOR: &str = "@@";
impl ServiceInfo {
    pub(crate) fn new(key: String) -> Result<Self> {
        let max_index = 2;
        let cluster_index = 2;
        let service_name_index = 1;
        let group_index = 0;

        let keys: Vec<_> = key.split(SERVICE_INFO_SEPARATOR).collect();

        if key.len() > max_index {
            Ok(ServiceInfo {
                group_name: keys[group_index].to_owned(),
                name: keys[service_name_index].to_owned(),
                clusters: keys[cluster_index].to_owned(),
                ..Default::default()
            })
        } else if keys.len() == max_index {
            Ok(ServiceInfo {
                group_name: keys[group_index].to_owned(),
                name: keys[service_name_index].to_owned(),
                ..Default::default()
            })
        } else {
            Err(GroupNameParseErr(
                "group name must not be null!".to_string(),
            ))
        }
    }

    pub fn expired(&self) -> bool {
        let now = SystemTime::now();
        let now = now.elapsed();
        if now.is_err() {
            return true;
        }
        let now = now.unwrap().as_millis();

        now - self.last_ref_time as u128 > self.cache_millis as u128
    }

    pub fn ip_count(&self) -> i32 {
        if self.hosts.is_none() {
            return 0;
        }
        self.hosts.as_ref().unwrap().len() as i32
    }

    pub fn validate(&self) -> bool {
        if self.all_ips {
            return true;
        }

        if self.hosts.is_none() {
            return false;
        }

        let hosts = self.hosts.as_ref().unwrap();
        for host in hosts {
            if !host.healthy {
                continue;
            }

            if host.weight > 0 as f64 {
                return true;
            }
        }

        false
    }

    pub fn get_grouped_service_name(service_name: &str, group_name: &str) -> String {
        if !group_name.is_empty() && !service_name.contains(SERVICE_INFO_SEPARATOR) {
            let service_name = format!("{}{}{}", &group_name, SERVICE_INFO_SEPARATOR, service_name);
            return service_name;
        }
        service_name.to_string()
    }

    pub fn hosts_to_json(&self) -> String {
        if self.hosts.is_none() {
            return "".to_string();
        }
        let json = serde_json::to_string(self.hosts.as_ref().unwrap());
        if let Err(e) = json {
            error!("hosts to json failed. {:?}", e);
            return "".to_string();
        }
        json.unwrap()
    }

    pub fn get_key(name: &str, clusters: &str) -> String {
        if !clusters.is_empty() {
            let key = format!("{}{}{}", name, SERVICE_INFO_SEPARATOR, clusters);
            return key;
        }

        name.to_string()
    }
}

impl Default for ServiceInfo {
    fn default() -> Self {
        Self {
            name: Default::default(),
            group_name: Default::default(),
            clusters: Default::default(),
            cache_millis: 1000,
            last_ref_time: 0,
            checksum: Default::default(),
            all_ips: false,
            reach_protection_threshold: false,
            hosts: Default::default(),
        }
    }
}

pub trait InstanceChooser {
    fn choose(self) -> Option<ServiceInstance>;
}

pub trait NamingEventListener: Send + Sync + 'static {
    fn event(&self, service_info: Arc<ServiceInfo>);
}

pub trait NamingService {
    fn register_service(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()>;

    fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()>;

    fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()>;

    fn get_all_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Vec<ServiceInstance>>;

    fn select_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Vec<ServiceInstance>>;

    fn select_one_healthy_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<ServiceInstance>;

    fn get_service_list(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)>;

    fn subscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()>;

    fn unsubscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        event_listener: Arc<dyn NamingEventListener>,
    ) -> Result<()>;
}

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

    /// Set [`plugin::AuthPlugin`]
    pub fn with_auth_plugin(mut self, auth_plugin: Arc<dyn plugin::AuthPlugin>) -> Self {
        self.auth_plugin = Some(auth_plugin);
        self
    }

    pub fn build(self) -> Result<impl NamingService> {
        let auth_plugin = match self.auth_plugin {
            None => Arc::new(plugin::NoopAuthPlugin::default()),
            Some(plugin) => plugin,
        };
        NacosNamingService::new(self.client_props, auth_plugin)
    }

    pub async fn build_async(self) -> Result<impl NamingService> {
        let auth_plugin = match self.auth_plugin {
            None => Arc::new(plugin::NoopAuthPlugin::default()),
            Some(plugin) => plugin,
        };
        NacosNamingService::new(self.client_props, auth_plugin)
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

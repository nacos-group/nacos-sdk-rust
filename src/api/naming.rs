use std::{collections::HashMap, sync::Arc};

use crate::{api::error::Result, naming::NacosNamingService};
use futures::Future;
use serde::{Deserialize, Serialize};

use super::{events::Subscriber, props::ClientProps};

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

pub trait InstanceChooser {
    fn choose(self) -> Option<ServiceInstance>;
}

pub type AsyncFuture<T> = Box<dyn Future<Output = Result<T>> + Send + Unpin + 'static>;
pub trait NamingService {
    fn register_service(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()>;

    fn register_service_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> AsyncFuture<()>;

    fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()>;

    fn deregister_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> AsyncFuture<()>;

    fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()>;

    fn batch_register_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> AsyncFuture<()>;

    fn get_all_instances(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<Vec<ServiceInstance>>;

    fn get_all_instances_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> AsyncFuture<Vec<ServiceInstance>>;

    fn select_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> Result<Vec<ServiceInstance>>;

    fn select_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
        healthy: bool,
    ) -> AsyncFuture<Vec<ServiceInstance>>;

    fn select_one_healthy_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> Result<ServiceInstance>;

    fn select_one_healthy_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscribe: bool,
    ) -> AsyncFuture<ServiceInstance>;

    fn get_service_list(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> Result<(Vec<String>, i32)>;

    fn get_service_list_async(
        &self,
        page_no: i32,
        page_size: i32,
        group_name: Option<String>,
    ) -> AsyncFuture<(Vec<String>, i32)>;

    fn subscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscriber: Arc<dyn Subscriber>,
    ) -> Result<()>;

    fn subscribe_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscriber: Arc<dyn Subscriber>,
    ) -> AsyncFuture<()>;

    fn unsubscribe(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscriber: Arc<dyn Subscriber>,
    ) -> Result<()>;

    fn unsubscribe_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        clusters: Vec<String>,
        subscriber: Arc<dyn Subscriber>,
    ) -> AsyncFuture<()>;
}

pub struct NamingServiceBuilder {
    client_props: ClientProps,
}

impl NamingServiceBuilder {
    pub fn new(client_props: ClientProps) -> Self {
        NamingServiceBuilder { client_props }
    }

    pub fn build(self) -> Result<impl NamingService> {
        NacosNamingService::new(self.client_props)
    }

    pub async fn build_async(self) -> Result<impl NamingService> {
        NacosNamingService::new(self.client_props)
    }
}

impl Default for NamingServiceBuilder {
    fn default() -> Self {
        NamingServiceBuilder {
            client_props: ClientProps::new(),
        }
    }
}

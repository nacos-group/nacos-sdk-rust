use std::collections::HashMap;

use crate::{api::error::Result, naming::NacosNamingService};
use futures::Future;
use serde::{Deserialize, Serialize};

use super::props::ClientProps;

const DEFAULT_CLUSTER_NAME: &str = "DEFAULT";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ServiceInstance {
    instance_id: Option<String>,

    ip: String,

    port: i32,

    weight: f64,

    healthy: bool,

    enabled: bool,

    ephemeral: bool,

    cluster_name: Option<String>,

    service_name: Option<String>,

    metadata: HashMap<String, String>,
}

impl ServiceInstance {
    pub fn new(ip: String, port: i32) -> Self {
        ServiceInstance {
            ip,
            port,
            instance_id: None,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some(DEFAULT_CLUSTER_NAME.to_owned()),
            service_name: None,
            metadata: HashMap::new(),
        }
    }

    pub fn instance_id(mut self, instance_id: String) -> Self {
        self.instance_id = Some(instance_id);
        self
    }

    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }

    pub fn cluster_name(mut self, cluster_name: String) -> Self {
        self.cluster_name = Some(cluster_name);
        self
    }

    pub fn service_name(mut self, service_name: String) -> Self {
        self.service_name = Some(service_name);
        self
    }

    pub fn add_meta_data(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
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
}

pub struct NamingServiceBuilder {
    client_props: ClientProps,
}

impl NamingServiceBuilder {
    pub fn new(client_props: ClientProps) -> Self {
        NamingServiceBuilder { client_props }
    }

    pub fn build(self) -> impl NamingService {
        NacosNamingService::new(self.client_props)
    }

    pub async fn build_async(self) -> impl NamingService {
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

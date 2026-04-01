#![allow(dead_code)]

use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse};
use nacos_sdk::api::naming::{NamingChangeEvent, NamingEventListener, ServiceInstance};
use std::collections::HashMap;
use std::sync::Arc;

/// Builder for [`ServiceInstance`] with sensible defaults.
///
/// # Examples
///
/// ```
/// use nacos_sdk::api::naming::ServiceInstance;
///
/// let instance = ServiceInstanceBuilder::new()
///     .ip("192.168.1.1")
///     .port(8080)
///     .cluster("DEFAULT")
///     .metadata("env", "test")
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct ServiceInstanceBuilder {
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

impl Default for ServiceInstanceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceInstanceBuilder {
    /// Creates a new builder with sensible defaults:
    /// - IP: `127.0.0.1`
    /// - Port: `8080`
    /// - Weight: `1.0`
    /// - Healthy: `true`
    /// - Enabled: `true`
    /// - Ephemeral: `true`
    /// - Cluster: `DEFAULT`
    pub fn new() -> Self {
        Self {
            ip: "127.0.0.1".to_string(),
            port: 8080,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some("DEFAULT".to_string()),
            service_name: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the instance IP address.
    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = ip.into();
        self
    }

    /// Set the instance port.
    pub fn port(mut self, port: i32) -> Self {
        self.port = port;
        self
    }

    /// Set the instance weight (0.0 - 1.0).
    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// Set whether the instance is healthy.
    pub fn healthy(mut self, healthy: bool) -> Self {
        self.healthy = healthy;
        self
    }

    /// Set whether the instance is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set whether the instance is ephemeral.
    pub fn ephemeral(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }

    /// Set the cluster name.
    pub fn cluster(mut self, cluster: impl Into<String>) -> Self {
        self.cluster_name = Some(cluster.into());
        self
    }

    /// Set the service name.
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Add a metadata key-value pair.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Build the [`ServiceInstance`].
    pub fn build(self) -> ServiceInstance {
        ServiceInstance {
            instance_id: None,
            ip: self.ip,
            port: self.port,
            weight: self.weight,
            healthy: self.healthy,
            enabled: self.enabled,
            ephemeral: self.ephemeral,
            cluster_name: self.cluster_name,
            service_name: self.service_name,
            metadata: self.metadata,
        }
    }
}

/// Mock [`ConfigChangeListener`] for testing config change notifications.
///
/// Collects all received [`ConfigResponse`] instances for later inspection.
///
/// # Examples
///
/// ```
/// use nacos_sdk::api::config::ConfigChangeListener;
/// use std::sync::Arc;
///
/// let listener = Arc::new(MockConfigListener::new());
/// // ... add listener to config service ...
/// let notifications = listener.get_notifications();
/// ```
pub struct MockConfigListener {
    pub notifications: std::sync::Mutex<Vec<ConfigResponse>>,
}

impl MockConfigListener {
    /// Creates a new empty listener.
    pub fn new() -> Self {
        Self {
            notifications: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Returns a snapshot of all received notifications.
    pub fn get_notifications(&self) -> Vec<ConfigResponse> {
        self.notifications.lock().expect("mutex poisoned").clone()
    }
}

impl ConfigChangeListener for MockConfigListener {
    fn notify(&self, config_resp: ConfigResponse) {
        self.notifications
            .lock()
            .expect("mutex poisoned")
            .push(config_resp);
    }
}

impl Default for MockConfigListener {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock [`NamingEventListener`] for testing naming service events.
///
/// Collects all received [`NamingChangeEvent`] instances for later inspection.
///
/// # Examples
///
/// ```
/// use nacos_sdk::api::naming::NamingEventListener;
/// use std::sync::Arc;
///
/// let listener = Arc::new(MockNamingListener::new());
/// // ... subscribe with naming service ...
/// let events = listener.get_events();
/// ```
pub struct MockNamingListener {
    pub events: std::sync::Mutex<Vec<Arc<NamingChangeEvent>>>,
}

impl MockNamingListener {
    /// Creates a new empty listener.
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Returns a snapshot of all received events.
    pub fn get_events(&self) -> Vec<Arc<NamingChangeEvent>> {
        self.events.lock().expect("mutex poisoned").clone()
    }
}

impl NamingEventListener for MockNamingListener {
    fn event(&self, event: Arc<NamingChangeEvent>) {
        self.events.lock().expect("mutex poisoned").push(event);
    }
}

impl Default for MockNamingListener {
    fn default() -> Self {
        Self::new()
    }
}

/// Test configuration data generator.
///
/// Provides random, unique test data for config service integration tests.
///
/// # Examples
///
/// ```
/// let config = ConfigTestData::random();
/// assert!(config.data_id.starts_with("test-config-"));
/// ```
#[derive(Clone, Debug)]
pub struct ConfigTestData {
    pub data_id: String,
    pub group: String,
    pub namespace: String,
    pub content: String,
    pub content_type: String,
}

impl ConfigTestData {
    /// Generates random test data with a unique `data_id` and `content`.
    pub fn random() -> Self {
        let id: u32 = rand::random();
        Self {
            data_id: format!("test-config-{}", id),
            group: "TEST_GROUP".to_string(),
            namespace: "public".to_string(),
            content: format!("test_content_{}", id),
            content_type: "text".to_string(),
        }
    }

    /// Generates random test data with a custom group.
    pub fn random_with_group(group: impl Into<String>) -> Self {
        let mut data = Self::random();
        data.group = group.into();
        data
    }

    /// Generates random test data with a custom namespace.
    pub fn random_with_namespace(namespace: impl Into<String>) -> Self {
        let mut data = Self::random();
        data.namespace = namespace.into();
        data
    }

    pub fn json() -> Self {
        let id: u32 = rand::random();
        Self {
            data_id: format!("test-json-config-{}", id),
            group: "TEST_GROUP".to_string(),
            namespace: "public".to_string(),
            content: format!(r#"{{"key":"value-{}","count":{}}}"#, id, id),
            content_type: "json".to_string(),
        }
    }

    pub fn yaml() -> Self {
        let id: u32 = rand::random();
        Self {
            data_id: format!("test-yaml-config-{}", id),
            group: "TEST_GROUP".to_string(),
            namespace: "public".to_string(),
            content: format!("key: value-{}\ncount: {}", id, id),
            content_type: "yaml".to_string(),
        }
    }
}

use std::collections::HashMap;

/// Configures settings for Client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// server_addr like 127.0.0.1:9848
    pub(crate) server_addr: String,
    pub(crate) namespace: String,
    /// app_name
    pub(crate) app_name: Option<String>,
    /// metadata
    pub(crate) labels: HashMap<String, String>,
}

impl ClientConfig {
    /// Creates a new `ClientConfig`.
    pub fn new() -> Self {
        ClientConfig {
            server_addr: String::from(crate::api::constants::DEFAULT_SERVER_ADDR),
            /// public is "", Should define a more meaningful namespace
            namespace: String::from(""),
            app_name: None,
            labels: HashMap::default(),
        }
    }

    /// Sets the server addr.
    pub fn server_addr(mut self, server_addr: impl Into<String>) -> Self {
        self.server_addr = server_addr.into();
        self
    }

    /// Sets the namespace.
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Sets the app_name.
    pub fn app_name(mut self, app_name: impl Into<String>) -> Self {
        let name = app_name.into();
        self.app_name = Some(name.clone());
        self.labels
            .insert(crate::api::constants::KEY_LABEL_APP_NAME.to_string(), name);
        self
    }

    /// Sets the labels.
    pub fn labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.clone_from(&labels);
        self
    }
}

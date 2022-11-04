use crate::common::remote::generate_request_id;
use nacos_macro::request;
use serde::{Deserialize, Serialize};

/// ConfigBatchListenRequest from client.
#[request(identity = "ConfigBatchListenRequest", module = "config")]
pub(crate) struct ConfigBatchListenRequest {
    /// listen or remove-listen.
    pub(crate) listen: bool,
    /// context of listen.
    pub(crate) config_listen_contexts: Vec<ConfigListenContext>,
}

impl ConfigBatchListenRequest {
    pub fn new(listen: bool) -> Self {
        Self {
            listen,
            config_listen_contexts: Vec::new(),
            request_id: Some(generate_request_id()),
            ..Default::default()
        }
    }

    /// Set ConfigListenContext.
    pub fn config_listen_context(mut self, contexts: Vec<ConfigListenContext>) -> Self {
        self.config_listen_contexts = contexts;
        self
    }

    /// Add ConfigListenContext.
    pub fn add_config_listen_context(mut self, context: ConfigListenContext) -> Self {
        self.config_listen_contexts.push(context);
        self
    }
}

/// The Context of config listen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigListenContext {
    /// DataId
    #[serde(rename = "dataId")]
    data_id: String,
    /// Group
    group: String,
    /// tenant
    tenant: String,
    /// Md5
    md5: String,
}

impl ConfigListenContext {
    pub fn new(data_id: String, group: String, tenant: String, md5: String) -> Self {
        ConfigListenContext {
            data_id,
            group,
            tenant,
            md5,
        }
    }
}

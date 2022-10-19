#![allow(non_snake_case)]
use crate::common::remote::request::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigBatchListenClientRequest {
    requestId: String,
    /// could be empty.
    headers: HashMap<String, String>,
    /// listen or remove-listen.
    listen: bool,
    /// context of listen.
    configListenContexts: Vec<ConfigListenContext>,
}

impl Request for ConfigBatchListenClientRequest {
    fn request_id(&self) -> &String {
        &self.requestId
    }
    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn type_url(&self) -> &String {
        &TYPE_CONFIG_BATCH_LISTEN_CLIENT_REQUEST
    }
}

impl ConfigBatchListenClientRequest {
    pub fn new(listen: bool) -> Self {
        ConfigBatchListenClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
            listen,
            configListenContexts: Vec::new(),
        }
    }

    /// Set ConfigListenContext.
    pub fn config_listen_context(mut self, contexts: Vec<ConfigListenContext>) -> Self {
        self.configListenContexts = contexts;
        self
    }

    /// Add ConfigListenContext.
    pub fn add_config_listen_context(mut self, context: ConfigListenContext) -> Self {
        self.configListenContexts.push(context);
        self
    }
}

/// The Context of config listen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigListenContext {
    /// DataId
    dataId: String,
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
            dataId: data_id,
            group,
            tenant,
            md5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigQueryClientRequest {
    requestId: String,
    /// could be empty.
    headers: HashMap<String, String>,
    /// DataId
    dataId: String,
    /// Group
    group: String,
    /// tenant
    tenant: String,
}

impl Request for ConfigQueryClientRequest {
    fn request_id(&self) -> &String {
        &self.requestId
    }
    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn type_url(&self) -> &String {
        &TYPE_CONFIG_QUERY_CLIENT_REQUEST
    }
}

impl ConfigQueryClientRequest {
    pub fn new(data_id: String, group: String, tenant: String) -> Self {
        ConfigQueryClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
            dataId: data_id,
            group,
            tenant,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigRemoveClientRequest {
    requestId: String,
    /// could be empty.
    headers: HashMap<String, String>,
    /// DataId
    dataId: String,
    /// Group
    group: String,
    /// tenant
    tenant: String,
    /// tag
    tag: Option<String>,
}

impl Request for ConfigRemoveClientRequest {
    fn request_id(&self) -> &String {
        &self.requestId
    }
    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn type_url(&self) -> &String {
        &TYPE_CONFIG_REMOVE_CLIENT_REQUEST
    }
}

impl ConfigRemoveClientRequest {
    pub fn new(data_id: String, group: String, tenant: String) -> Self {
        ConfigRemoveClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
            dataId: data_id,
            group,
            tenant,
            tag: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigPublishClientRequest {
    requestId: String,
    /// could be empty.
    headers: HashMap<String, String>,
    /// DataId
    dataId: String,
    /// Group
    group: String,
    /// tenant
    tenant: String,
    /// content
    content: String,
    /// Cas md5 (prev content's md5)
    casMd5: Option<String>,
    /// Addition Map
    additionMap: HashMap<String, String>,
}

impl Request for ConfigPublishClientRequest {
    fn request_id(&self) -> &String {
        &self.requestId
    }
    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn type_url(&self) -> &String {
        &TYPE_CONFIG_PUBLISH_CLIENT_REQUEST
    }
}

impl ConfigPublishClientRequest {
    pub fn new(data_id: String, group: String, tenant: String, content: String) -> Self {
        ConfigPublishClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
            dataId: data_id,
            group,
            tenant,
            content,
            casMd5: None,
            additionMap: HashMap::default(),
        }
    }

    /// Sets the cas_md5.
    pub fn cas_md5(mut self, cas_md5: Option<String>) -> Self {
        self.casMd5 = cas_md5;
        self
    }

    /// Add into additionMap.
    pub fn add_addition_param(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.additionMap.insert(key.into(), val.into());
    }

    /// Add into additionMap.
    pub fn add_addition_params(&mut self, addition_params: HashMap<String, String>) {
        self.additionMap.extend(addition_params.into_iter());
    }
}

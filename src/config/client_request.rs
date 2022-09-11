#![allow(non_snake_case)]
use crate::common::remote::request::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigBatchListenClientRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
    /// listen or remove-listen.
    listen: bool,
    /// context of listen.
    configListenContexts: Vec<ConfigListenContext>,
}

impl Request for ConfigBatchListenClientRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
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
    /// count be empty.
    headers: HashMap<String, String>,
    /// DataId
    dataId: String,
    /// Group
    group: String,
    /// tenant
    tenant: String,
}

impl Request for ConfigQueryClientRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
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

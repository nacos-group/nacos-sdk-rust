#![allow(non_snake_case)]
use crate::common::remote::request::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigChangeNotifyServerRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
    pub(crate) dataId: String,
    pub(crate) group: String,
    /// "public" or "" maybe None, must be careful about compatibility
    pub(crate) tenant: Option<String>,
}

impl Request for ConfigChangeNotifyServerRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_CONFIG_CHANGE_NOTIFY_SERVER_REQUEST
    }
}

impl ConfigChangeNotifyServerRequest {
    /// Sets the headers.
    pub fn headers(self, headers: HashMap<String, String>) -> Self {
        ConfigChangeNotifyServerRequest { headers, ..self }
    }
}

impl From<&str> for ConfigChangeNotifyServerRequest {
    fn from(json_str: &str) -> Self {
        let de: serde_json::Result<Self> = serde_json::from_str(json_str);
        de.unwrap()
    }
}

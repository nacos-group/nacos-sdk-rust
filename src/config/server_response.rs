#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigChangeBatchListenServerResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
    changedConfigs: Option<Vec<ConfigContext>>,
}

impl Response for ConfigChangeBatchListenServerResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
    }

    fn request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
    }

    fn message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn error_code(&self) -> u32 {
        self.errorCode
    }

    fn type_url(&self) -> &String {
        &TYPE_CONFIG_CHANGE_BATCH_LISTEN_RESPONSE
    }
}

impl ConfigChangeBatchListenServerResponse {
    pub fn new(request_id: String) -> Self {
        ConfigChangeBatchListenServerResponse {
            requestId: Some(request_id),
            resultCode: ResponseCode::Ok,
            errorCode: 0,
            message: None,
            changedConfigs: None,
        }
    }

    pub fn changed_configs(&self) -> Option<&Vec<ConfigContext>> {
        Option::from(&self.changedConfigs)
    }
}

impl From<&str> for ConfigChangeBatchListenServerResponse {
    fn from(json_str: &str) -> Self {
        let de: serde_json::Result<Self> = serde_json::from_str(json_str);
        de.unwrap()
    }
}

/// The Context of config changed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigContext {
    /// DataId
    pub(crate) dataId: String,
    /// Group
    pub(crate) group: String,
    /// tenant
    pub(crate) tenant: String,
}

const CONFIG_NOT_FOUND: u32 = 300;
const CONFIG_QUERY_CONFLICT: u32 = 400;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigQueryServerResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,

    /// json, properties, txt, html, xml, ...
    contentType: String,
    content: String,
    md5: String,
    /// whether content was encrypted with encryptedDataKey.
    encryptedDataKey: Option<String>,

    /// now is useless.
    tag: Option<String>,
    lastModified: i64,
    beta: bool,
}

impl Response for ConfigQueryServerResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
    }

    fn request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
    }

    fn message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn error_code(&self) -> u32 {
        self.errorCode
    }

    fn type_url(&self) -> &String {
        &TYPE_CONFIG_CHANGE_BATCH_LISTEN_RESPONSE
    }
}

impl ConfigQueryServerResponse {
    pub(crate) fn is_not_found(&self) -> bool {
        self.errorCode == CONFIG_NOT_FOUND
    }
    pub(crate) fn is_query_conflict(&self) -> bool {
        self.errorCode == CONFIG_QUERY_CONFLICT
    }
    pub fn content_type(&self) -> &String {
        &self.contentType
    }
    pub fn content(&self) -> &String {
        &self.content
    }
    pub fn md5(&self) -> &String {
        &self.md5
    }
    pub fn encrypted_Data_Key(&self) -> Option<&String> {
        Option::from(&self.encryptedDataKey)
    }
    pub fn last_modified(&self) -> i64 {
        self.lastModified
    }
}

impl From<&str> for ConfigQueryServerResponse {
    fn from(json_str: &str) -> Self {
        let de: serde_json::Result<Self> = serde_json::from_str(json_str);
        de.unwrap()
    }
}

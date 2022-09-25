#![allow(non_snake_case)]
use crate::common::remote::request::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConnectResetServerRequest {
    requestId: String,
    /// could be empty.
    headers: HashMap<String, String>,
    serverIp: Option<String>,
    serverPort: Option<String>,
}

impl Request for ConnectResetServerRequest {
    fn request_id(&self) -> &String {
        &self.requestId
    }
    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn type_url(&self) -> &String {
        &TYPE_CONNECT_RESET_SERVER_REQUEST
    }
}

impl ConnectResetServerRequest {
    /// Sets the headers.
    pub fn headers(self, headers: HashMap<String, String>) -> Self {
        ConnectResetServerRequest { headers, ..self }
    }
}

impl From<&str> for ConnectResetServerRequest {
    fn from(json_str: &str) -> Self {
        let de: serde_json::Result<Self> = serde_json::from_str(json_str);
        de.unwrap()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ClientDetectionServerRequest {
    requestId: String,
    /// could be empty.
    headers: HashMap<String, String>,
}

impl Request for ClientDetectionServerRequest {
    fn request_id(&self) -> &String {
        &self.requestId
    }
    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn type_url(&self) -> &String {
        &TYPE_CLIENT_DETECTION_SERVER_REQUEST
    }
}

impl ClientDetectionServerRequest {
    /// Sets the headers.
    pub fn headers(self, headers: HashMap<String, String>) -> Self {
        ClientDetectionServerRequest { headers, ..self }
    }
}

impl From<&str> for ClientDetectionServerRequest {
    fn from(json_str: &str) -> Self {
        let de: serde_json::Result<Self> = serde_json::from_str(json_str);
        de.unwrap()
    }
}

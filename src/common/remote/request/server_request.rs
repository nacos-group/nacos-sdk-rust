#![allow(non_snake_case)]
use crate::common::remote::request::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ConnectResetServerRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
    serverIp: Option<String>,
    serverPort: Option<String>,
}

impl Request for ConnectResetServerRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_CONNECT_RESET_SERVER_REQUEST
    }
}

impl ConnectResetServerRequest {
    pub fn new(server_ip: Option<String>, server_port: Option<String>) -> Self {
        ConnectResetServerRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
            serverIp: server_ip,
            serverPort: server_port,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ClientDetectionServerRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
}

impl Request for ClientDetectionServerRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_CLIENT_DETECTION_SERVER_REQUEST
    }
}

impl ClientDetectionServerRequest {
    pub fn new() -> Self {
        ClientDetectionServerRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
        }
    }
}

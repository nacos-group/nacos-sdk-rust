#![allow(non_snake_case)]
use crate::common::remote::request::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ServerCheckClientRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
}

impl Request for ServerCheckClientRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_SERVER_CHECK_CLIENT_REQUEST
    }
}

impl ServerCheckClientRequest {
    pub fn new() -> Self {
        ServerCheckClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConnectionSetupClientRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
    clientVersion: String,
    tenant: String,
    /// count be empty.
    labels: HashMap<String, String>,
}

impl Request for ConnectionSetupClientRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_CONNECT_SETUP_CLIENT_REQUEST
    }
}

impl ConnectionSetupClientRequest {
    pub fn new(tenant: String, labels: HashMap<String, String>) -> Self {
        ConnectionSetupClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
            clientVersion: String::from("2.1.0"),
            tenant,
            labels,
        }
    }

    /// Sets the labels against.
    pub fn labels(self, labels: HashMap<String, String>) -> Self {
        ConnectionSetupClientRequest { labels, ..self }
    }
}

/// HealthCheck from client, default keep alive time 5s.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HealthCheckClientRequest {
    requestId: String,
    /// count be empty.
    headers: HashMap<String, String>,
}

impl Request for HealthCheckClientRequest {
    fn get_request_id(&self) -> &String {
        &self.requestId
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_HEALTH_CHECK_CLIENT_REQUEST
    }
}

impl HealthCheckClientRequest {
    pub fn new() -> Self {
        HealthCheckClientRequest {
            requestId: generate_request_id(),
            headers: HashMap::new(),
        }
    }
}

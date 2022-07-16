#![allow(non_snake_case)]
use crate::common::remote::request::{
    generate_request_id, Request, TYPE_SERVER_CHECK_CLIENT_REQUEST,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
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

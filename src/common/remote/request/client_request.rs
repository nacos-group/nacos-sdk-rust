use std::collections::HashMap;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::common::remote::request::{
    generate_request_id, Request, TYPE_SERVER_CHECK_CLIENT_REQUEST,
};

struct ClientRequest {
    pub(crate) request_id: String,
    pub(crate) headers: HashMap<String, String>,
}

impl ClientRequest {
    pub fn new() -> Self {
        ClientRequest {
            request_id: generate_request_id(),
            headers: HashMap::new(),
        }
    }
}

pub(crate) struct ServerCheckClientRequest {
    base_client_request: ClientRequest,
}

impl Request for ServerCheckClientRequest {
    fn get_request_id(&self) -> &String {
        &self.base_client_request.request_id
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.base_client_request.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_SERVER_CHECK_CLIENT_REQUEST
    }
}

impl ServerCheckClientRequest {
    pub fn new() -> Self {
        ServerCheckClientRequest {
            base_client_request: ClientRequest::new(),
        }
    }
}

impl Serialize for ServerCheckClientRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("ServerCheckClientRequest", 2)?;
        s.serialize_field("requestId", &self.base_client_request.request_id)?;
        s.serialize_field("headers", &self.base_client_request.headers)?;
        s.end()
    }
}

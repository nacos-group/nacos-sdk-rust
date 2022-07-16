use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::common::remote::response::Response;

struct ServerResponse {
    pub(crate) request_id: String,
    pub(crate) message: Option<String>,
    pub(crate) error_code: i32,
}

impl ServerResponse {
    pub fn new(request_id: String) -> Self {
        ServerResponse {
            request_id,
            message: None,
            error_code: 0,
        }
    }
}

pub(crate) struct ServerCheckServerResponse {
    base_server_response: ServerResponse,
    connection_id: String,
}

impl Response for ServerCheckServerResponse {
    fn get_request_id(&self) -> &String {
        &self.base_server_response.request_id
    }

    fn get_message(&self) -> Option<&String> {
        Option::from(&self.base_server_response.message)
    }

    fn get_error_code(&self) -> i32 {
        self.base_server_response.error_code
    }
}

impl ServerCheckServerResponse {
    pub fn new(connection_id: String, request_id: String) -> Self {
        ServerCheckServerResponse {
            base_server_response: ServerResponse::new(request_id),
            connection_id,
        }
    }
}

impl Serialize for ServerCheckServerResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("ServerCheckServerResponse", 2)?;
        s.serialize_field("requestId", &self.base_server_response.request_id)?;
        s.serialize_field("connectionId", &self.connection_id)?;
        s.end()
    }
}

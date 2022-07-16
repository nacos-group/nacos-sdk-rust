#![allow(non_snake_case)]
use crate::common::remote::response::{Response, TYPE_SERVER_CHECK_SERVER_RESPONSE};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct ServerCheckServerResponse {
    requestId: String,
    message: Option<String>,
    errorCode: i32,
    /// only ServerCheckServerResponse return it.
    connectionId: String,
}

impl Response for ServerCheckServerResponse {
    fn get_connection_id(&self) -> &String {
        &self.connectionId
    }

    fn get_request_id(&self) -> &String {
        &self.requestId
    }

    fn get_message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn get_error_code(&self) -> i32 {
        self.errorCode
    }

    fn get_type_url(&self) -> &String {
        &TYPE_SERVER_CHECK_SERVER_RESPONSE
    }
}

impl ServerCheckServerResponse {
    pub fn new(connection_id: String, request_id: String) -> Self {
        ServerCheckServerResponse {
            requestId: request_id,
            connectionId: connection_id,
            message: None,
            errorCode: 0,
        }
    }
}

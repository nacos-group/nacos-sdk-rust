#![allow(non_snake_case)]
use crate::common::remote::response::{Response, ResponseCode, TYPE_SERVER_CHECK_SERVER_RESPONSE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ServerCheckServerResponse {
    /// only ServerCheckServerResponse return it.
    connectionId: String,
    requestId: String,
    resultCode: ResponseCode,
    errorCode: i32,
    message: Option<String>,
}

impl Response for ServerCheckServerResponse {
    fn is_success(&self) -> bool {
        ResponseCode::SUCCESS == self.resultCode
    }

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
            connectionId: connection_id,
            requestId: request_id,
            resultCode: ResponseCode::SUCCESS,
            errorCode: 0,
            message: None,
        }
    }
}

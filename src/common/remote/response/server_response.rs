#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ServerCheckServerResponse {
    /// only ServerCheckServerResponse return it.
    connectionId: String,
    requestId: Option<String>,
    resultCode: i32,
    errorCode: i32,
    message: Option<String>,
}

impl Response for ServerCheckServerResponse {
    fn is_success(&self) -> bool {
        200 == self.resultCode
    }

    fn get_connection_id(&self) -> Option<&String> {
        Option::from(&self.connectionId)
    }

    fn get_request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
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
            requestId: Some(request_id),
            resultCode: 200,
            errorCode: 0,
            message: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ErrorResponse {
    requestId: Option<String>,
    resultCode: i32,
    errorCode: i32,
    message: Option<String>,
}

impl Response for ErrorResponse {
    fn is_success(&self) -> bool {
        200 == self.resultCode
    }

    fn get_connection_id(&self) -> Option<&String> {
        None
    }

    fn get_request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
    }

    fn get_message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn get_error_code(&self) -> i32 {
        self.errorCode
    }

    fn get_type_url(&self) -> &String {
        &TYPE_ERROR_SERVER_RESPONSE
    }
}

impl ErrorResponse {
    pub fn new(request_id: String) -> Self {
        ErrorResponse {
            requestId: Some(request_id),
            resultCode: 500,
            errorCode: 0,
            message: None,
        }
    }
}

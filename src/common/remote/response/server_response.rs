#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ServerCheckServerResponse {
    /// only ServerCheckServerResponse return it.
    connectionId: String,
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
}

impl Response for ServerCheckServerResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
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

    fn get_error_code(&self) -> u32 {
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
            resultCode: ResponseCode::Ok,
            errorCode: 0,
            message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ErrorResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
}

impl Response for ErrorResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
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

    fn get_error_code(&self) -> u32 {
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
            resultCode: ResponseCode::Fail,
            errorCode: 500,
            message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HealthCheckServerResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
}

impl Response for HealthCheckServerResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
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

    fn get_error_code(&self) -> u32 {
        self.errorCode
    }

    fn get_type_url(&self) -> &String {
        &TYPE_HEALTH_CHECK_SERVER_RESPONSE
    }
}

impl HealthCheckServerResponse {
    pub fn new(request_id: String) -> Self {
        HealthCheckServerResponse {
            requestId: Some(request_id),
            resultCode: ResponseCode::Ok,
            errorCode: 0,
            message: None,
        }
    }
}

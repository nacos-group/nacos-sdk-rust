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

    fn connection_id(&self) -> Option<&String> {
        Option::from(&self.connectionId)
    }

    fn request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
    }

    fn message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn error_code(&self) -> u32 {
        self.errorCode
    }

    fn type_url(&self) -> &String {
        &TYPE_SERVER_CHECK_SERVER_RESPONSE
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ErrorResponse {
    requestId: Option<String>,
    /// [`ResponseCode::Fail`]
    resultCode: ResponseCode,
    /// 500 or else
    errorCode: u32,
    message: Option<String>,
}

impl Response for ErrorResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
    }

    fn request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
    }

    fn message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn error_code(&self) -> u32 {
        self.errorCode
    }

    fn type_url(&self) -> &String {
        &TYPE_ERROR_SERVER_RESPONSE
    }
}

impl From<&str> for ErrorResponse {
    fn from(json_str: &str) -> Self {
        let de: serde_json::Result<Self> = serde_json::from_str(json_str);
        de.unwrap()
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

    fn request_id(&self) -> Option<&String> {
        Option::from(&self.requestId)
    }

    fn message(&self) -> Option<&String> {
        Option::from(&self.message)
    }

    fn error_code(&self) -> u32 {
        self.errorCode
    }

    fn type_url(&self) -> &String {
        &TYPE_HEALTH_CHECK_SERVER_RESPONSE
    }
}

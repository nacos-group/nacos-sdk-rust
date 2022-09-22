#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConnectResetClientResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
}

impl Response for ConnectResetClientResponse {
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
        &TYPE_CONNECT_RESET_CLIENT_RESPONSE
    }
}

impl ConnectResetClientResponse {
    pub fn new(request_id: String) -> Self {
        ConnectResetClientResponse {
            requestId: Some(request_id),
            resultCode: ResponseCode::Ok,
            errorCode: 0,
            message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ClientDetectionClientResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
}

impl Response for ClientDetectionClientResponse {
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
        &TYPE_CLIENT_DETECTION_CLIENT_RESPONSE
    }
}

impl ClientDetectionClientResponse {
    pub fn new(request_id: String) -> Self {
        ClientDetectionClientResponse {
            requestId: Some(request_id),
            resultCode: ResponseCode::Ok,
            errorCode: 0,
            message: None,
        }
    }
}

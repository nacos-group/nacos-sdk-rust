#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConnectResetClientResponse {
    requestId: Option<String>,
    resultCode: i32,
    errorCode: i32,
    message: Option<String>,
}

impl Response for ConnectResetClientResponse {
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
        &TYPE_CONNECT_RESET_CLIENT_RESPONSE
    }
}

impl ConnectResetClientResponse {
    pub fn new(request_id: String) -> Self {
        ConnectResetClientResponse {
            requestId: Some(request_id),
            resultCode: 200,
            errorCode: 0,
            message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ClientDetectionClientResponse {
    requestId: Option<String>,
    resultCode: i32,
    errorCode: i32,
    message: Option<String>,
}

impl Response for ClientDetectionClientResponse {
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
        &TYPE_CLIENT_DETECTION_CLIENT_RESPONSE
    }
}

impl ClientDetectionClientResponse {
    pub fn new(request_id: String) -> Self {
        ClientDetectionClientResponse {
            requestId: Some(request_id),
            resultCode: 200,
            errorCode: 0,
            message: None,
        }
    }
}

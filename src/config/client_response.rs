#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigChangeNotifyClientResponse {
    requestId: Option<String>,
    resultCode: ResponseCode,
    errorCode: u32,
    message: Option<String>,
}

impl Response for ConfigChangeNotifyClientResponse {
    fn is_success(&self) -> bool {
        ResponseCode::Ok == self.resultCode
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
        &TYPE_CONFIG_CHANGE_NOTIFY_CLIENT_RESPONSE
    }
}

impl ConfigChangeNotifyClientResponse {
    pub fn new(request_id: String) -> Self {
        ConfigChangeNotifyClientResponse {
            requestId: Some(request_id),
            resultCode: ResponseCode::Ok,
            errorCode: 0,
            message: None,
        }
    }
}

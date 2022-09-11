#![allow(non_snake_case)]
use crate::common::remote::response::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ConfigChangeNotifyClientResponse {
    requestId: Option<String>,
    resultCode: i32,
    errorCode: i32,
    message: Option<String>,
}

impl Response for ConfigChangeNotifyClientResponse {
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
        &TYPE_CONFIG_CHANGE_NOTIFY_CLIENT_RESPONSE
    }
}

impl ConfigChangeNotifyClientResponse {
    pub fn new(request_id: String) -> Self {
        ConfigChangeNotifyClientResponse {
            requestId: Some(request_id),
            resultCode: 200,
            errorCode: 0,
            message: None,
        }
    }
}

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

pub(crate) mod server_response;

pub(crate) trait Response {
    fn is_success(&self) -> bool;
    fn get_connection_id(&self) -> &String;
    fn get_request_id(&self) -> &String;
    fn get_message(&self) -> Option<&String>;
    fn get_error_code(&self) -> i32;
    fn get_type_url(&self) -> &String;
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub(crate) enum ResponseCode {
    /// Request success. (200, "Response ok").
    SUCCESS,
    /// Request failed. (500, "Response fail").
    FAIL,
}

lazy_static! {
    pub static ref TYPE_SERVER_CHECK_SERVER_RESPONSE: String =
        String::from("com.alibaba.nacos.api.remote.response.ServerCheckResponse");
}

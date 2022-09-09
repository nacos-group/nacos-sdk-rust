use lazy_static::lazy_static;

pub(crate) mod server_response;

pub(crate) trait Response {
    fn is_success(&self) -> bool;
    fn get_connection_id(&self) -> Option<&String>;
    fn get_request_id(&self) -> Option<&String>;
    fn get_message(&self) -> Option<&String>;
    fn get_error_code(&self) -> i32;
    fn get_type_url(&self) -> &String;
}

lazy_static! {
    /// com.alibaba.nacos.api.remote.response.ServerCheckResponse
    pub static ref TYPE_SERVER_CHECK_SERVER_RESPONSE: String = String::from("ServerCheckResponse");

    /// com.alibaba.nacos.api.remote.response.ErrorResponse
    pub static ref TYPE_ERROR_SERVER_RESPONSE: String = String::from("ErrorResponse");
}

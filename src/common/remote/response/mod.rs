use lazy_static::lazy_static;

pub(crate) mod server_response;

pub(crate) trait Response {
    fn get_request_id(&self) -> &String;
    fn get_message(&self) -> Option<&String>;
    fn get_error_code(&self) -> i32;
}

lazy_static! {
    static ref TYPE_SERVER_CHECK_SERVER_RESPONSE: String =
        String::from("com.alibaba.nacos.api.remote.response.ServerCheckResponse");
}

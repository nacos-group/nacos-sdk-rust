use lazy_static::lazy_static;

pub(crate) mod client_response;
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
    /// com.alibaba.nacos.api.remote.response.ErrorResponse
    pub static ref TYPE_ERROR_SERVER_RESPONSE: String = String::from("ErrorResponse");

    /// com.alibaba.nacos.api.remote.response.ServerCheckResponse
    pub static ref TYPE_SERVER_CHECK_SERVER_RESPONSE: String = String::from("ServerCheckResponse");

    /// com.alibaba.nacos.api.remote.response.HealthCheckResponse
    pub static ref TYPE_HEALTH_CHECK_SERVER_RESPONSE: String = String::from("HealthCheckResponse");

    /// com.alibaba.nacos.api.remote.response.ConnectResetResponse
    pub static ref TYPE_CONNECT_RESET_CLIENT_RESPONSE: String = String::from("ConnectResetResponse");

    /// com.alibaba.nacos.api.remote.response.ClientDetectionResponse
    pub static ref TYPE_CLIENT_DETECTION_CLIENT_RESPONSE: String = String::from("ClientDetectionResponse");

    // --- config client resp ---
    /// com.alibaba.nacos.api.config.remote.response.ConfigChangeNotifyResponse
    pub static ref TYPE_CONFIG_CHANGE_NOTIFY_CLIENT_RESPONSE: String = String::from("ConfigChangeNotifyResponse");

    // --- config server resp ---
    /// com.alibaba.nacos.api.config.remote.response.ConfigChangeBatchListenResponse
    pub static ref TYPE_CONFIG_CHANGE_BATCH_LISTEN_RESPONSE: String = String::from("ConfigChangeBatchListenResponse");

}

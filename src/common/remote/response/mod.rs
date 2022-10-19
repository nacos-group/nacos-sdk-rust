use lazy_static::lazy_static;

pub(crate) mod client_response;
pub(crate) mod server_response;

pub(crate) trait Response {
    fn is_success(&self) -> bool;
    fn connection_id(&self) -> Option<&String> {
        None
    }
    fn request_id(&self) -> Option<&String>;
    fn message(&self) -> Option<&String>;
    fn error_code(&self) -> u32;
    fn type_url(&self) -> &String;
}

#[derive(Debug, Clone, PartialEq, serde_repr::Serialize_repr, serde_repr::Deserialize_repr)]
#[repr(u32)]
pub enum ResponseCode {
    /**
     * SUCCESS(200, "Response ok").
     */
    Ok = 200,
    /**
     * FAIL(500, "Response fail").
     */
    Fail = 500,
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

    /// com.alibaba.nacos.api.config.remote.response.ConfigQueryResponse
    pub static ref TYPE_CONFIG_QUERY_SERVER_RESPONSE: String = String::from("ConfigQueryResponse");

    /// com.alibaba.nacos.api.config.remote.response.ConfigRemoveResponse
    pub static ref TYPE_CONFIG_REMOVE_SERVER_RESPONSE: String = String::from("ConfigRemoveResponse");

}

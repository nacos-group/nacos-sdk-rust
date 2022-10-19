use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

pub(crate) mod client_request;
pub(crate) mod server_request;

pub(crate) trait Request {
    fn request_id(&self) -> &String;
    fn headers(&self) -> &HashMap<String, String>;
    fn type_url(&self) -> &String;
}

lazy_static! {
    pub static ref LOCAL_IP: String = local_ipaddress::get().unwrap();

    /// com.alibaba.nacos.api.remote.request.ServerCheckRequest
    pub static ref TYPE_SERVER_CHECK_CLIENT_REQUEST: String = String::from("ServerCheckRequest");

    /// com.alibaba.nacos.api.remote.request.ConnectionSetupRequest
    pub static ref TYPE_CONNECT_SETUP_CLIENT_REQUEST: String = String::from("ConnectionSetupRequest");

    /// com.alibaba.nacos.api.remote.request.HealthCheckRequest
    pub static ref TYPE_HEALTH_CHECK_CLIENT_REQUEST: String = String::from("HealthCheckRequest");

    /// com.alibaba.nacos.api.remote.request.ConnectResetRequest
    pub static ref TYPE_CONNECT_RESET_SERVER_REQUEST: String = String::from("ConnectResetRequest");

    /// com.alibaba.nacos.api.remote.request.ClientDetectionRequest
    pub static ref TYPE_CLIENT_DETECTION_SERVER_REQUEST: String = String::from("ClientDetectionRequest");

    // --- config server req ---
    /// com.alibaba.nacos.api.config.remote.request.ConfigChangeNotifyRequest
    pub static ref TYPE_CONFIG_CHANGE_NOTIFY_SERVER_REQUEST: String = String::from("ConfigChangeNotifyRequest");

    // --- config client req ---
    /// com.alibaba.nacos.api.config.remote.request.ConfigBatchListenRequest
    pub static ref TYPE_CONFIG_BATCH_LISTEN_CLIENT_REQUEST: String = String::from("ConfigBatchListenRequest");

    /// com.alibaba.nacos.api.config.remote.request.ConfigQueryRequest
    pub static ref TYPE_CONFIG_QUERY_CLIENT_REQUEST: String = String::from("ConfigQueryRequest");

    /// com.alibaba.nacos.api.config.remote.request.ConfigRemoveRequest
    pub static ref TYPE_CONFIG_REMOVE_CLIENT_REQUEST: String = String::from("ConfigRemoveRequest");

}

// odd by client request id.
const SEQUENCE_INITIAL_VALUE: i64 = 1;
const SEQUENCE_DELTA: i64 = 2;
static ATOMIC_SEQUENCE: AtomicI64 = AtomicI64::new(SEQUENCE_INITIAL_VALUE);

pub(crate) fn generate_request_id() -> String {
    let seq = ATOMIC_SEQUENCE.fetch_add(SEQUENCE_DELTA, Ordering::Relaxed);
    if seq > i64::MAX - 1000 {
        ATOMIC_SEQUENCE.store(SEQUENCE_INITIAL_VALUE, Ordering::SeqCst);
    }
    seq.to_string()
}

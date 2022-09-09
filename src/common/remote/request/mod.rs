use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

pub(crate) mod client_request;
pub(crate) mod server_request;

pub(crate) trait Request {
    fn get_request_id(&self) -> &String;
    fn get_headers(&self) -> &HashMap<String, String>;
    fn get_type_url(&self) -> &String;
}

lazy_static! {
    // TODO get local_ip;
    pub static ref LOCAL_IP: String = String::from("127.0.0.1");

    /// com.alibaba.nacos.api.remote.request.ServerCheckRequest
    pub static ref TYPE_SERVER_CHECK_CLIENT_REQUEST: String = String::from("ServerCheckRequest");

    /// com.alibaba.nacos.api.remote.request.ConnectionSetupRequest
    pub static ref TYPE_CONNECT_SETUP_CLIENT_REQUEST: String = String::from("ConnectionSetupRequest");

    /// com.alibaba.nacos.api.remote.request.ConnectResetRequest
    pub static ref TYPE_CONNECT_RESET_SERVER_REQUEST: String = String::from("ConnectResetRequest");

    /// com.alibaba.nacos.api.remote.request.ClientDetectionRequest
    pub static ref TYPE_CLIENT_DETECTION_SERVER_REQUEST: String = String::from("ClientDetectionRequest");

}

// odd by client request id.
const SEQUENCE_INITIAL_VALUE: i64 = 1;
const SEQUENCE_DELTA: i64 = 2;
static ATOMIC_SEQUENCE: AtomicI64 = AtomicI64::new(SEQUENCE_INITIAL_VALUE);

fn generate_request_id() -> String {
    let seq = ATOMIC_SEQUENCE.fetch_add(SEQUENCE_DELTA, Ordering::Relaxed);
    if seq > i64::MAX - 1000 {
        ATOMIC_SEQUENCE.store(SEQUENCE_INITIAL_VALUE, Ordering::SeqCst);
    }
    seq.to_string()
}

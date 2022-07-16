use lazy_static::lazy_static;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

lazy_static! {
    // TODO get local_ip;
    pub static ref LOCAL_IP: String = String::from("127.0.0.1");

    static ref TYPE_SERVER_CHECK_CLIENT_REQUEST: String =
        String::from("com.alibaba.nacos.api.remote.request.ServerCheckRequest");

}

// odd by client request id.
const SEQUENCE_INITIAL_VALUE: i64 = 1;
const SEQUENCE_DELTA: i64 = 2;
static ATOMIC_SEQUENCE: AtomicI64 = AtomicI64::new(SEQUENCE_INITIAL_VALUE);

pub(crate) trait Request {
    fn get_request_id(&self) -> &String;
    fn get_headers(&self) -> &HashMap<String, String>;
    fn get_type_url(&self) -> &String;
}

fn generate_request_id() -> String {
    let seq = ATOMIC_SEQUENCE.fetch_add(SEQUENCE_DELTA, Ordering::Relaxed);
    if seq > i64::MAX - 1000 {
        ATOMIC_SEQUENCE.store(SEQUENCE_INITIAL_VALUE, Ordering::SeqCst);
    }
    seq.to_string()
}

struct ClientRequest {
    pub(crate) request_id: String,
    pub(crate) headers: HashMap<String, String>,
}

impl ClientRequest {
    pub fn new() -> Self {
        ClientRequest {
            request_id: generate_request_id(),
            headers: HashMap::new(),
        }
    }
}

pub(super) struct ServerCheckClientRequest {
    base_client_request: ClientRequest,
}

impl Request for ServerCheckClientRequest {
    fn get_request_id(&self) -> &String {
        &self.base_client_request.request_id
    }
    fn get_headers(&self) -> &HashMap<String, String> {
        &self.base_client_request.headers
    }
    fn get_type_url(&self) -> &String {
        &TYPE_SERVER_CHECK_CLIENT_REQUEST
    }
}

impl ServerCheckClientRequest {
    pub fn new() -> Self {
        ServerCheckClientRequest {
            base_client_request: ClientRequest::new(),
        }
    }
}

impl Serialize for ServerCheckClientRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("ServerCheckClientRequest", 1)?;
        s.serialize_field("requestId", &self.base_client_request.request_id)?;
        s.serialize_field("headers", &self.base_client_request.headers)?;
        s.end()
    }
}

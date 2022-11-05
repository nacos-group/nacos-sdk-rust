pub mod grpc;

use std::sync::atomic::{AtomicI64, Ordering};

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

/// make address's port plus 1000
pub(crate) fn into_grpc_server_addr(address: &str) -> String {
    let v: Vec<&str> = address.split(':').collect();
    let port = v.get(1).unwrap().parse::<u32>().unwrap() + 1000;
    format!("{}:{}", v.first().unwrap(), port)
}

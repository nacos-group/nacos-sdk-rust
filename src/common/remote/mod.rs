pub mod grpc;
pub mod server_list;

use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

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

/// A shared, lazily-initialized `reqwest::Client` instance.
///
/// `reqwest::Client` internally manages a connection pool; reusing a single instance
/// avoids wasted resources and improves performance.
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client initialization should succeed")
});

pub(crate) fn http_client() -> &'static reqwest::Client {
    &HTTP_CLIENT
}

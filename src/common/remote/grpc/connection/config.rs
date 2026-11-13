use std::collections::HashMap;
use std::time::Duration;

use super::backoff::RetryPolicy;

/// Tunables for a managed connection: timeouts, health checking and the
/// reconnect backoff policy.
#[allow(dead_code)] // consumed by the connection supervisor
#[derive(Clone, Debug)]
pub(crate) struct ConnectionSettings {
    pub(crate) client_version: String,
    pub(crate) namespace: String,
    pub(crate) labels: HashMap<String, String>,
    pub(crate) server_check_timeout: Duration,
    pub(crate) capability_negotiation_timeout: Duration,
    pub(crate) request_timeout: Duration,
    pub(crate) health_interval: Duration,
    pub(crate) health_retry_times: u32,
    pub(crate) health_timeout: Duration,
    pub(crate) backlog: usize,
    pub(crate) retry: RetryPolicy,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            client_version: String::new(),
            namespace: String::new(),
            labels: HashMap::new(),
            server_check_timeout: Duration::from_secs(3),
            capability_negotiation_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
            health_interval: Duration::from_secs(5),
            health_retry_times: 3,
            health_timeout: Duration::from_secs(3),
            backlog: 1024,
            retry: RetryPolicy::default(),
        }
    }
}

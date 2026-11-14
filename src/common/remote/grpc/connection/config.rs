use std::collections::HashMap;
use std::time::Duration;

use super::backoff::RetryPolicy;

/// Tunables for a managed connection: timeouts, health checking and the
/// reconnect backoff policy.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings_are_pinned() {
        let settings = ConnectionSettings::default();
        assert_eq!(settings.client_version, "");
        assert_eq!(settings.namespace, "");
        assert!(settings.labels.is_empty());
        assert_eq!(settings.server_check_timeout, Duration::from_secs(3));
        assert_eq!(
            settings.capability_negotiation_timeout,
            Duration::from_secs(5)
        );
        assert_eq!(settings.request_timeout, Duration::from_secs(10));
        assert_eq!(settings.health_interval, Duration::from_secs(5));
        assert_eq!(settings.health_retry_times, 3);
        assert_eq!(settings.health_timeout, Duration::from_secs(3));
        assert_eq!(settings.backlog, 1024);
        assert_eq!(
            settings.retry.delay_with_roll(0, 0),
            Duration::from_millis(250)
        );
        assert_eq!(settings.retry.stable_reset_after(), Duration::from_secs(10));
    }
}

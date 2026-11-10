use std::time::Duration;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_HTTP2_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(360);
const DEFAULT_HTTP2_KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub(crate) struct GrpcConfiguration {
    pub(crate) host: String,
    pub(crate) port: Option<u16>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) tcp_nodelay: bool,
    pub(crate) http2_keep_alive_interval: Option<Duration>,
    pub(crate) http2_keep_alive_timeout: Option<Duration>,
}

impl Default for GrpcConfiguration {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: Default::default(),
            timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            // tonic defaults this to `true`; the client keeps Nagle's
            // algorithm enabled, so it must be set explicitly.
            tcp_nodelay: false,
            http2_keep_alive_interval: Some(DEFAULT_HTTP2_KEEP_ALIVE_INTERVAL),
            http2_keep_alive_timeout: Some(DEFAULT_HTTP2_KEEP_ALIVE_TIMEOUT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_request_timeout_is_ten_seconds() {
        assert_eq!(
            GrpcConfiguration::default().timeout,
            Some(Duration::from_secs(10))
        );
    }

    #[test]
    fn test_default_http2_keepalive_settings() {
        // The HTTP/2 keepalive interval and timeout have non-None defaults and
        // are applied to the tonic endpoint. They must survive the removal of
        // the unused configuration fields, otherwise connections would silently
        // lose their keepalive behavior.
        let config = GrpcConfiguration::default();
        assert_eq!(
            config.http2_keep_alive_interval,
            Some(Duration::from_secs(360))
        );
        assert_eq!(
            config.http2_keep_alive_timeout,
            Some(Duration::from_secs(20))
        );
    }

    #[test]
    fn test_default_host_and_port() {
        // `host` is overwritten from the server list and `port` is derived
        // from it, but both remain part of the configuration contract.
        let config = GrpcConfiguration::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, None);
    }
}

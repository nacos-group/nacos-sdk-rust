use std::time::Duration;

use reqwest::header::HeaderValue;
use tonic::transport::Uri;

#[derive(Clone)]
pub(crate) struct GrpcConfiguration {
    pub(crate) host: String,
    pub(crate) port: Option<u32>,
    pub(crate) origin: Option<Uri>,
    pub(crate) user_agent: Option<HeaderValue>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) concurrency_limit: Option<usize>,
    pub(crate) rate_limit: Option<(u64, Duration)>,
    pub(crate) init_stream_window_size: Option<u32>,
    pub(crate) init_connection_window_size: Option<u32>,
    pub(crate) tcp_keepalive: Option<Duration>,
    pub(crate) tcp_nodelay: bool,
    pub(crate) http2_keep_alive_interval: Option<Duration>,
    pub(crate) http2_keep_alive_timeout: Option<Duration>,
    pub(crate) http2_keep_alive_while_idle: Option<bool>,
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) http2_adaptive_window: Option<bool>,
}

#[allow(dead_code)]
impl GrpcConfiguration {
    /// Set origin from a URI string. Returns self only if parsing succeeds.
    pub(crate) fn with_origin(mut self, uri: &str) -> Self {
        if let Ok(uri) = uri.parse::<Uri>() {
            self.origin = Some(uri);
        }
        self
    }

    /// Set user agent from a string. Returns self only if parsing succeeds.
    pub(crate) fn with_user_agent(mut self, ua: String) -> Self {
        if let Ok(ua) = HeaderValue::try_from(ua) {
            self.user_agent = Some(ua);
        }
        self
    }
}

impl Default for GrpcConfiguration {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: Default::default(),
            origin: Default::default(),
            user_agent: Default::default(),
            timeout: Default::default(),
            concurrency_limit: Default::default(),
            rate_limit: Default::default(),
            init_stream_window_size: Default::default(),
            init_connection_window_size: Default::default(),
            tcp_keepalive: Default::default(),
            tcp_nodelay: Default::default(),
            http2_keep_alive_interval: Default::default(),
            http2_keep_alive_timeout: Default::default(),
            http2_keep_alive_while_idle: Default::default(),
            connect_timeout: Default::default(),
            http2_adaptive_window: Default::default(),
        }
    }
}

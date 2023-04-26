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

impl GrpcConfiguration {
    pub(crate) fn with_host(mut self, host: String) -> Self {
        self.host = host;
        self
    }

    pub(crate) fn with_port(mut self, port: u32) -> Self {
        self.port = Some(port);
        self
    }

    pub(crate) fn with_origin(mut self, uri: &str) -> Self {
        let uri = uri.parse::<Uri>();
        if uri.is_err() {
            return self;
        }
        self.origin = Some(uri.unwrap());
        self
    }

    pub(crate) fn with_user_agent(mut self, ua: String) -> Self {
        let ua = HeaderValue::try_from(ua);
        if ua.is_err() {
            return self;
        }
        let ua = ua.unwrap();
        self.user_agent = Some(ua);
        self
    }

    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn with_concurrency_limit(mut self, concurrency_limit: usize) -> Self {
        self.concurrency_limit = Some(concurrency_limit);
        self
    }

    pub(crate) fn with_rate_limit(mut self, rate_limit: (u64, Duration)) -> Self {
        self.rate_limit = Some(rate_limit);
        self
    }

    pub(crate) fn with_init_stream_window_size(mut self, init_stream_window_size: u32) -> Self {
        self.init_stream_window_size = Some(init_stream_window_size);
        self
    }

    pub(crate) fn with_init_connection_window_size(
        mut self,
        init_connection_window_size: u32,
    ) -> Self {
        self.init_connection_window_size = Some(init_connection_window_size);
        self
    }

    pub(crate) fn with_tcp_keepalive(mut self, tcp_keepalive: Duration) -> Self {
        self.tcp_keepalive = Some(tcp_keepalive);
        self
    }

    pub(crate) fn with_tcp_nodelay(mut self, tcp_nodelay: bool) -> Self {
        self.tcp_nodelay = tcp_nodelay;
        self
    }

    pub(crate) fn with_http2_keep_alive_interval(
        mut self,
        http2_keep_alive_interval: Duration,
    ) -> Self {
        self.http2_keep_alive_interval = Some(http2_keep_alive_interval);
        self
    }

    pub(crate) fn with_http2_keep_alive_timeout(
        mut self,
        http2_keep_alive_timeout: Duration,
    ) -> Self {
        self.http2_keep_alive_timeout = Some(http2_keep_alive_timeout);
        self
    }

    pub(crate) fn with_http2_keep_alive_while_idle(
        mut self,
        http2_keep_alive_while_idle: bool,
    ) -> Self {
        self.http2_keep_alive_while_idle = Some(http2_keep_alive_while_idle);
        self
    }

    pub(crate) fn with_connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = Some(connect_timeout);
        self
    }

    pub(crate) fn with_http2_adaptive_window(mut self, http2_adaptive_window: bool) -> Self {
        self.http2_adaptive_window = Some(http2_adaptive_window);
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

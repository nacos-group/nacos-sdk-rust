use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures::{Future, StreamExt};
use tonic::transport::{Channel, Endpoint, Uri};
use tower::Service;
use tracing::{Instrument, debug, debug_span, error};

use super::config::GrpcConfiguration;
use super::connection::{Connector, Transport};
use super::nacos_grpc_service::GrpcStream;
use crate::api::error::Error;
use crate::api::error::Error::NoAvailableServer;
use crate::common::remote::server_list::ServerAddress;
use crate::nacos_proto::v2::{
    Payload, bi_request_stream_client::BiRequestStreamClient, request_client::RequestClient,
};

#[derive(Clone)]
pub(crate) struct Tonic {
    request_client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
    server_address: Arc<dyn ServerAddress>,
    endpoint_uri: Uri,
}

impl Tonic {
    pub fn new(grpc_config: GrpcConfiguration, server_address: Arc<dyn ServerAddress>) -> Self {
        let url_authority = if let Some(port) = grpc_config.port {
            format!("{}:{}", grpc_config.host, port)
        } else {
            grpc_config.host.clone()
        };

        let scheme = if cfg!(feature = "tls") {
            "https"
        } else {
            "http"
        };

        let endpoint_uri = Uri::builder()
            .scheme(scheme)
            .authority(url_authority)
            .path_and_query("/")
            .build()
            .expect("Endpoint URI construction should not fail with valid inputs");

        debug!("create new endpoint :{}", endpoint_uri);
        let endpoint = build_endpoint(&grpc_config, endpoint_uri.clone());

        let channel = endpoint.connect_lazy();

        let request_client = RequestClient::new(channel.clone());

        let bi_client = BiRequestStreamClient::new(channel);

        Self {
            request_client,
            bi_client,
            server_address,
            endpoint_uri,
        }
    }

    /// Fails fast when the server this transport was built for has been marked
    /// unavailable, instead of issuing a request that can only fail.
    fn ensure_server_available(&self) -> Result<(), Error> {
        if self.server_address.is_available() {
            return Ok(());
        }
        error!(
            "the server address {}:{} is not available",
            self.server_address.host(),
            self.server_address.port()
        );
        Err(NoAvailableServer)
    }
}

/// Builds the tonic endpoint from the gRPC configuration.
///
/// Split out from [`Tonic::new`] so the transport settings it applies can be
/// asserted directly.
fn build_endpoint(grpc_config: &GrpcConfiguration, endpoint_uri: Uri) -> Endpoint {
    let mut endpoint = Endpoint::from(endpoint_uri);

    if let Some(timeout) = grpc_config.timeout {
        endpoint = endpoint.timeout(timeout);
    }

    if let Some(http2_keep_alive_interval) = grpc_config.http2_keep_alive_interval {
        endpoint = endpoint.http2_keep_alive_interval(http2_keep_alive_interval);
    }

    if let Some(http2_keep_alive_timeout) = grpc_config.http2_keep_alive_timeout {
        endpoint = endpoint.keep_alive_timeout(http2_keep_alive_timeout);
    }

    // tonic defaults tcp_nodelay to true; the client keeps Nagle's algorithm
    // enabled, so it must be set explicitly.
    endpoint = endpoint.tcp_nodelay(grpc_config.tcp_nodelay);

    #[cfg(feature = "tls")]
    {
        use tonic::transport::{Certificate, ClientTlsConfig};

        let mut tls_config = ClientTlsConfig::new();
        if let Ok(ca_cert_path) = std::env::var(crate::api::constants::ENV_NACOS_CLIENT_TLS_CA_CERT)
        {
            let ca_cert_pem = std::fs::read_to_string(&ca_cert_path).unwrap_or_else(|e| {
                panic!(
                    "Failed to read TLS CA certificate file '{}': {}",
                    ca_cert_path, e
                )
            });
            tls_config = tls_config.ca_certificate(Certificate::from_pem(ca_cert_pem));
        }
        endpoint = endpoint
            .tls_config(tls_config)
            .expect("Failed to configure TLS on gRPC endpoint");
    }

    endpoint
}

pub(crate) struct TonicBuilder<S> {
    grpc_config: GrpcConfiguration,
    server_list: S,
}

impl<S> TonicBuilder<S>
where
    S: Service<(), Response = Arc<dyn ServerAddress>, Error = Error>,
    S::Future: Send + 'static,
{
    pub(crate) fn new(grpc_config: GrpcConfiguration, server_list: S) -> Self {
        Self {
            grpc_config,
            server_list,
        }
    }
}

impl<S> Service<()> for TonicBuilder<S>
where
    S: Service<(), Response = Arc<dyn ServerAddress>, Error = Error>,
    S::Future: Send + 'static,
{
    type Response = Tonic;

    type Error = Error;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _span_enter = debug_span!("tonic_builder").entered();
        self.server_list.poll_ready(cx)
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let _span_enter = debug_span!("tonic_builder").entered();

        let server_info_fut = self.server_list.call(());
        let mut grpc_config = self.grpc_config.clone();

        let tonic_fut = async move {
            let server_address = server_info_fut.await?;

            if grpc_config.port.is_none() {
                grpc_config.port = Some(server_address.port() + 1000);
            }
            grpc_config.host = server_address.host();

            let tonic = Tonic::new(grpc_config, server_address);
            Ok(tonic)
        }
        .in_current_span();
        Box::pin(tonic_fut)
    }
}

#[async_trait]
impl Transport for Tonic {
    async fn unary(&self, payload: Payload) -> Result<Payload, Error> {
        self.ensure_server_available()?;
        let mut client = self.request_client.clone();
        client
            .request(payload)
            .await
            .map(|response| response.into_inner())
            .map_err(Error::TonicGrpcStatus)
    }

    async fn open_stream(
        &self,
        request: GrpcStream<Payload>,
    ) -> Result<GrpcStream<Result<Payload, Error>>, Error> {
        self.ensure_server_available()?;
        let mut client = self.bi_client.clone();
        let response = client
            .request_bi_stream(request)
            .await
            .map_err(Error::TonicGrpcStatus)?;
        let stream = response
            .into_inner()
            .map(|item| item.map_err(Error::TonicGrpcStatus))
            .boxed();
        Ok(GrpcStream::new(stream))
    }

    fn server_addr(&self) -> String {
        self.endpoint_uri.to_string()
    }
}

#[async_trait]
impl<S> Connector for TonicBuilder<S>
where
    S: Service<(), Response = Arc<dyn ServerAddress>, Error = Error> + Send + 'static,
    S::Future: Send + 'static,
{
    async fn connect(&mut self) -> Result<Arc<dyn Transport>, Error> {
        futures::future::poll_fn(|cx| self.poll_ready(cx)).await?;
        let tonic = self.call(()).await?;
        Ok(Arc::new(tonic))
    }
}

#[cfg(test)]
mod endpoint_config_tests {
    use super::*;

    fn uri() -> Uri {
        Uri::from_static("http://127.0.0.1:8848")
    }

    #[test]
    fn test_endpoint_keeps_nagle_algorithm_enabled() {
        // tonic defaults tcp_nodelay to true. The client explicitly keeps
        // Nagle's algorithm enabled (tcp_nodelay = false), so the endpoint
        // must not inherit tonic's default.
        let endpoint = build_endpoint(&GrpcConfiguration::default(), uri());
        assert!(!endpoint.get_tcp_nodelay());
    }
}

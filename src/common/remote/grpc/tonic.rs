use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{Future, StreamExt};
use tonic::transport::{Channel, Endpoint, Uri};
use tower::{Service, layer::util::Stack};
use tracing::{Instrument, Span, debug, debug_span, error};

use crate::{
    common::remote::grpc::nacos_grpc_service::DynamicBiStreamingCallLayerWrapper,
    nacos_proto::v2::{
        Payload, bi_request_stream_client::BiRequestStreamClient, request_client::RequestClient,
    },
};

use super::{
    config::GrpcConfiguration,
    nacos_grpc_service::{Callback, NacosGrpcCall},
};
use super::{
    nacos_grpc_service::{
        BiStreamingCallIdentityLayer, DynamicBiStreamingCallLayer, DynamicBiStreamingCallService,
        DynamicUnaryCallLayer, DynamicUnaryCallLayerWrapper, DynamicUnaryCallService, GrpcStream,
        UnaryCallIdentityLayer,
    },
    server_address::ServerAddress,
};
use crate::api::error::Error;
use crate::api::error::Error::NoAvailableServer;
use crate::api::error::Error::TonicGrpcStatus;

#[derive(Clone)]
pub(crate) struct Tonic {
    channel: Channel,
    request_client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
    unary_call_layer: DynamicUnaryCallLayer,
    bi_call_layer: DynamicBiStreamingCallLayer,
    endpoint_uri: Uri,
    server_address: Arc<dyn ServerAddress>,
}

impl Tonic {
    pub fn new(
        grpc_config: GrpcConfiguration,
        unary_call_layer: DynamicUnaryCallLayer,
        bi_call_layer: DynamicBiStreamingCallLayer,
        server_address: Arc<dyn ServerAddress>,
    ) -> Self {
        let url_authority = if let Some(port) = grpc_config.port {
            format!("{}:{}", grpc_config.host, port)
        } else {
            grpc_config.host
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
        let mut endpoint = Endpoint::from(endpoint_uri.clone());

        if let Some(origin) = grpc_config.origin {
            endpoint = endpoint.origin(origin);
        }

        if let Some(user_agent) = grpc_config.user_agent {
            endpoint = endpoint
                .user_agent(user_agent)
                .expect("User agent should be settable with valid input");
        }

        if let Some(timeout) = grpc_config.timeout {
            endpoint = endpoint.timeout(timeout);
        }

        if let Some(concurrency_limit) = grpc_config.concurrency_limit {
            endpoint = endpoint.concurrency_limit(concurrency_limit);
        }

        if let Some(rate_limit) = grpc_config.rate_limit {
            endpoint = endpoint.rate_limit(rate_limit.0, rate_limit.1);
        }

        if let Some(init_stream_window_size) = grpc_config.init_stream_window_size {
            endpoint = endpoint.initial_stream_window_size(init_stream_window_size);
        }

        if let Some(init_connection_window_size) = grpc_config.init_connection_window_size {
            endpoint = endpoint.initial_connection_window_size(init_connection_window_size);
        }

        if let Some(http2_keep_alive_interval) = grpc_config.http2_keep_alive_interval {
            endpoint = endpoint.http2_keep_alive_interval(http2_keep_alive_interval);
        }

        if let Some(http2_keep_alive_timeout) = grpc_config.http2_keep_alive_timeout {
            endpoint = endpoint.http2_keep_alive_interval(http2_keep_alive_timeout);
        }

        if let Some(http2_keep_alive_while_idle) = grpc_config.http2_keep_alive_while_idle {
            endpoint = endpoint.keep_alive_while_idle(http2_keep_alive_while_idle);
        }

        if let Some(http2_adaptive_window) = grpc_config.http2_adaptive_window {
            endpoint = endpoint.http2_adaptive_window(http2_adaptive_window);
        }

        if let Some(connect_time) = grpc_config.connect_timeout {
            endpoint = endpoint.connect_timeout(connect_time);
        }

        endpoint = endpoint.tcp_nodelay(grpc_config.tcp_nodelay);
        endpoint = endpoint.tcp_keepalive(grpc_config.tcp_keepalive);

        let channel = endpoint.connect_lazy();

        let request_client = RequestClient::new(channel.clone());

        let bi_client = BiRequestStreamClient::new(channel.clone());

        Self {
            channel,
            request_client,
            bi_client,
            unary_call_layer,
            bi_call_layer,
            endpoint_uri,
            server_address,
        }
    }
}

fn unary_request(
    mut service: DynamicUnaryCallService,
    request: (Payload, Callback<Result<Payload, Error>>),
) -> GrpcCallTask {
    let task = async move {
        let (payload, mut cb) = request;
        let is_ok = cb.can_send().await;
        if !is_ok {
            return Err(Error::ErrResult(
                "unary_request failed, callback can not invoke, receiver has been closed."
                    .to_string(),
            ));
        }
        let ready = futures::future::poll_fn(|cx| service.poll_ready(cx))
            .in_current_span()
            .await;
        if let Err(e) = ready {
            error!("unary request service is not ready.{e}");
            return Err(e);
        }

        let response = service.call(payload).in_current_span().await;
        let send_ret = cb.send(response).await;
        if let Err(e) = send_ret {
            error!(
                "unary_request failed, callback can not invoke, send error. {}",
                e
            );
            return Err(Error::ErrResult(
                "unary_request failed, callback can not invoke, send error.".to_string(),
            ));
        }
        Ok(())
    }
    .in_current_span();

    GrpcCallTask::new(Box::new(task))
}

fn bi_request(
    mut service: DynamicBiStreamingCallService,
    request: (
        GrpcStream<Payload>,
        Callback<Result<GrpcStream<Result<Payload, Error>>, Error>>,
    ),
) -> GrpcCallTask {
    let task = async move {
        let (stream, mut cb) = request;
        let is_ok = cb.can_send().await;
        if !is_ok {
            return Err(Error::ErrResult(
                "bi_request failed, callback can not invoke, receiver has been closed.".to_string(),
            ));
        }

        let ready = futures::future::poll_fn(|cx| service.poll_ready(cx))
            .in_current_span()
            .await;
        if let Err(e) = ready {
            error!("bi request service is not ready. {e}");
            return Err(e);
        }

        let response = service.call(stream).in_current_span().await;

        let send_ret = cb.send(response).await;
        if let Err(e) = send_ret {
            error!(
                "bi_request failed, callback can not invoke, send error. {}",
                e
            );
            return Err(Error::ErrResult(
                "bi_request failed, callback can not invoke, send error.".to_string(),
            ));
        }
        Ok(())
    }
    .in_current_span();

    GrpcCallTask::new(Box::new(task))
}

pub(crate) struct TonicBuilder<S> {
    grpc_config: GrpcConfiguration,
    server_list: S,
    unary_call_layer: DynamicUnaryCallLayer,
    bi_call_layer: DynamicBiStreamingCallLayer,
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
            unary_call_layer: Arc::new(UnaryCallIdentityLayer),
            bi_call_layer: Arc::new(BiStreamingCallIdentityLayer),
        }
    }

    pub(crate) fn unary_call_layer(self, layer: DynamicUnaryCallLayer) -> Self {
        let stack = Arc::new(Stack::new(
            DynamicUnaryCallLayerWrapper(layer),
            DynamicUnaryCallLayerWrapper(self.unary_call_layer),
        ));
        Self {
            unary_call_layer: stack,
            ..self
        }
    }

    pub(crate) fn bi_call_layer(self, layer: DynamicBiStreamingCallLayer) -> Self {
        let stack = Arc::new(Stack::new(
            DynamicBiStreamingCallLayerWrapper(layer),
            DynamicBiStreamingCallLayerWrapper(self.bi_call_layer),
        ));
        Self {
            bi_call_layer: stack,
            ..self
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
        let unary_call_layer = self.unary_call_layer.clone();
        let bi_call_layer = self.bi_call_layer.clone();

        let tonic_fut = async move {
            let server_address = server_info_fut.await?;

            if grpc_config.port.is_none() {
                grpc_config.port = Some(server_address.port() + 1000);
            }
            grpc_config.host = server_address.host();

            let tonic = Tonic::new(grpc_config, unary_call_layer, bi_call_layer, server_address);
            Ok(tonic)
        }
        .in_current_span();
        Box::pin(tonic_fut)
    }
}

impl Service<NacosGrpcCall> for Tonic {
    type Response = ();

    type Error = Error;

    type Future = GrpcCallTask;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if !self.server_address.is_available() {
            error!(
                "the server address {}:{} is not available",
                self.server_address.host(),
                self.server_address.port()
            );
            return Poll::Ready(Err(NoAvailableServer));
        }
        match self.channel.poll_ready(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::TonicGrpcTransport(e))),
        }
    }

    fn call(&mut self, call: NacosGrpcCall) -> Self::Future {
        match call {
            NacosGrpcCall::RequestService(request) => {
                let _span_enter =
                    debug_span!("tonic_unary", server = self.endpoint_uri.to_string()).entered();
                let unary_request_client = self.request_client.clone();
                let unary_call_service = UnaryCallService::new(unary_request_client);
                let dynamic_unary_call_service =
                    self.unary_call_layer.layer(Box::new(unary_call_service));
                unary_request(dynamic_unary_call_service, request)
            }
            NacosGrpcCall::BIRequestService(request) => {
                let _span_enter =
                    debug_span!("tonic_bi_stream", server = self.endpoint_uri.to_string())
                        .entered();
                let bi_request_client = self.bi_client.clone();
                let bi_call_service = BiStreamingCallService::new(bi_request_client);
                let dynamic_bi_call_service = self.bi_call_layer.layer(Box::new(bi_call_service));
                bi_request(dynamic_bi_call_service, request)
            }
        }
    }
}

type InnerTask = Box<dyn Future<Output = Result<(), Error>> + Send + 'static>;

pub(crate) struct GrpcCallTask {
    inner: InnerTask,
    span: Span,
}

impl GrpcCallTask {
    pub(crate) fn new(inner: InnerTask) -> Self {
        Self {
            inner,
            span: Span::current(),
        }
    }
}

impl Future for GrpcCallTask {
    type Output = Result<(), Error>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        let _enter = this.span.enter();

        let pin = unsafe { Pin::new_unchecked(this.inner.as_mut()) };
        let poll = pin.poll(cx);

        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(ret)) => Poll::Ready(Ok(ret)),
            Poll::Ready(Err(e)) => {
                error!("grpc call task get an error: {e}");
                Poll::Ready(Err(e))
            }
        }
    }
}

pub(crate) struct UnaryCallService {
    client: RequestClient<Channel>,
}

impl UnaryCallService {
    pub(crate) fn new(client: RequestClient<Channel>) -> Self {
        Self { client }
    }
}

impl Service<Payload> for UnaryCallService {
    type Response = Payload;

    type Error = Error;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Payload) -> Self::Future {
        let mut client = self.client.clone();
        let fut = async move {
            let response = client.request(req).in_current_span().await;
            match response {
                Ok(ret) => Ok(ret.into_inner()),
                Err(status) => Err(TonicGrpcStatus(Box::new(status))),
            }
        }
        .in_current_span();
        Box::pin(fut)
    }
}

pub(crate) struct BiStreamingCallService {
    client: BiRequestStreamClient<Channel>,
}

impl BiStreamingCallService {
    pub(crate) fn new(client: BiRequestStreamClient<Channel>) -> Self {
        Self { client }
    }
}

impl Service<GrpcStream<Payload>> for BiStreamingCallService {
    type Response = GrpcStream<Result<Payload, Error>>;

    type Error = Error;

    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: GrpcStream<Payload>) -> Self::Future {
        let mut client = self.client.clone();

        let fut = async move {
            let response = client.request_bi_stream(req).in_current_span().await;
            match response {
                Ok(response) => {
                    let response = response
                        .into_inner()
                        .map(|item| match item {
                            Ok(payload) => Ok(payload),
                            Err(status) => Err(TonicGrpcStatus(Box::new(status))),
                        })
                        .boxed();
                    Ok(GrpcStream::new(response))
                }
                Err(status) => Err(TonicGrpcStatus(Box::new(status))),
            }
        }
        .in_current_span();
        Box::pin(fut)
    }
}

#[cfg(test)]
fn assert_err_result(error: Error, expected_msg: &str) {
    match error {
        Error::ErrResult(msg) => assert_eq!(msg, expected_msg),
        _ => panic!(
            "expected ErrResult with '{}', got {:?}",
            expected_msg, error
        ),
    }
}

#[cfg(test)]
pub mod tonic_unary_call_tests {

    use crate::{nacos_proto::v2::Metadata, test_config};
    use mockall::*;
    use tokio::sync::oneshot;

    use super::assert_err_result;
    use super::*;

    mock! {
        UnaryCallTestService {}
        impl Service<Payload> for UnaryCallTestService {
            type Response = Payload;

            type Error = Error;

            type Future =
                Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

            fn poll_ready<'a>(&mut self, cx: &mut Context<'a>) -> Poll<Result<(), <Self as Service<Payload>>::Error>>;

            fn call(&mut self, req: Payload) -> <Self as Service<Payload>>::Future;

        }
    }

    fn setup() {
        test_config::setup_log();
    }

    fn teardown() {}

    fn run_test<T, F>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        setup();
        let ret = test();
        teardown();
        ret
    }

    // Helper functions
    fn mock_unary_service_ready() -> MockUnaryCallTestService {
        let mut test_service = MockUnaryCallTestService::new();
        test_service
            .expect_poll_ready()
            .returning(|_| Poll::Ready(Ok(())));
        test_service
            .expect_call()
            .returning(|request| Box::pin(async move { Ok(request) }));
        test_service
    }

    fn mock_unary_service_error(msg: &str) -> MockUnaryCallTestService {
        let msg = msg.to_string();
        let mut test_service = MockUnaryCallTestService::new();
        test_service
            .expect_poll_ready()
            .returning(|_| Poll::Ready(Ok(())));
        test_service.expect_call().returning(move |_| {
            Box::pin({
                let msg = msg.clone();
                async move { Err(Error::ErrResult(msg)) }
            })
        });
        test_service
    }

    fn mock_unary_service_not_ready(msg: &str) -> MockUnaryCallTestService {
        let msg = msg.to_string();
        let mut test_service = MockUnaryCallTestService::new();
        test_service
            .expect_poll_ready()
            .returning(move |_| Poll::Ready(Err(Error::ErrResult(msg.clone()))));
        test_service
            .expect_call()
            .returning(|request| Box::pin(async move { Ok(request) }));
        test_service
    }

    #[tokio::test]
    pub async fn test_unary_request_and_can_not_send() {
        run_test(|| async {
            let test_service = mock_unary_service_ready();
            let request = Payload::default();

            let (giver, taker) = want::new();
            let (tx, _rx) = oneshot::channel::<Result<Payload, Error>>();
            let callback = Callback::new(giver, tx);

            drop(taker);

            let ret = unary_request(Box::new(test_service), (request, callback)).await;
            assert!(ret.is_err());
            assert_err_result(
                ret.unwrap_err(),
                "unary_request failed, callback can not invoke, receiver has been closed.",
            );
        })
        .await;
    }

    #[tokio::test]
    pub async fn test_unary_request_and_response_send_error() {
        run_test(|| async {
            let test_service = mock_unary_service_ready();
            let request = Payload::default();

            let (giver, mut taker) = want::new();
            let (tx, rx) = oneshot::channel::<Result<Payload, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();
            drop(rx);

            let ret = unary_request(Box::new(test_service), (request, callback)).await;
            assert!(ret.is_err());
            assert_err_result(
                ret.unwrap_err(),
                "unary_request failed, callback can not invoke, send error.",
            );
        })
        .await;
    }

    #[tokio::test]
    pub async fn test_unary_request_and_request_service_not_ready() {
        run_test(|| async {
            let test_service = mock_unary_service_not_ready("test_service is not ready.");
            let request = Payload::default();

            let (giver, mut taker) = want::new();
            let (tx, _rx) = oneshot::channel::<Result<Payload, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();

            let ret = unary_request(Box::new(test_service), (request, callback)).await;
            assert!(ret.is_err());
            assert_err_result(ret.unwrap_err(), "test_service is not ready.");
        })
        .await;
    }

    #[tokio::test]
    pub async fn test_unary_request() {
        run_test(|| async {
            let test_service = mock_unary_service_ready();

            let mut request = Payload::default();
            let mut metadata = Metadata::default();
            metadata.r#type = "test_type".to_string();
            request.metadata = Some(metadata);

            let (giver, mut taker) = want::new();
            let (tx, rx) = oneshot::channel::<Result<Payload, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();

            let ret = unary_request(Box::new(test_service), (request, callback)).await;
            assert!(ret.is_ok());

            let response = rx.await.unwrap().unwrap();
            assert_eq!(response.metadata.unwrap().r#type, "test_type");
        })
        .await;
    }

    #[tokio::test]
    pub async fn test_unary_request_and_response_error() {
        run_test(|| async {
            let test_service = mock_unary_service_error("test-error");
            let request = Payload::default();

            let (giver, mut taker) = want::new();
            let (tx, rx) = oneshot::channel::<Result<Payload, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();

            let ret = unary_request(Box::new(test_service), (request, callback)).await;
            assert!(ret.is_ok());

            assert_err_result(rx.await.unwrap().unwrap_err(), "test-error");
        })
        .await;
    }
}

#[cfg(test)]
pub mod tonic_bi_call_tests {

    use crate::{nacos_proto::v2::Metadata, test_config};
    use async_stream::stream;
    use futures_util::stream;
    use mockall::*;
    use tokio::sync::oneshot;

    use super::assert_err_result;
    use super::*;

    mock! {
        BiCallTestService {}
        impl Service<GrpcStream<Payload>> for BiCallTestService {
            type Response = GrpcStream<Result<Payload, Error>>;

            type Error = Error;

            type Future =
                Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

            fn poll_ready<'a>(&mut self, cx: &mut Context<'a>) -> Poll<Result<(), <Self as Service<GrpcStream<Payload>>>::Error>>;

            fn call(&mut self, req: GrpcStream<Payload>) -> <Self as Service<GrpcStream<Payload>>>::Future;

        }
    }

    fn setup() {
        test_config::setup_log();
    }

    fn teardown() {}

    fn run_test<T, F>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        setup();
        let ret = test();
        teardown();
        ret
    }

    // Helper functions for bi-streaming tests
    fn mock_bi_service_echo() -> MockBiCallTestService {
        let mut test_service = MockBiCallTestService::new();
        test_service
            .expect_poll_ready()
            .returning(|_| Poll::Ready(Ok(())));
        test_service.expect_call().returning(|request_stream| {
            let response_stream = stream! {
                for await request in request_stream {
                    yield Ok(request);
                }
            };
            let grpc_stream = GrpcStream::new(Box::pin(response_stream));
            Box::pin(async move { Ok(grpc_stream) })
        });
        test_service
    }

    fn create_request_stream() -> GrpcStream<Payload> {
        let request_stream = stream::once(async move { Payload::default() });
        GrpcStream::new(Box::pin(request_stream))
    }

    #[tokio::test]
    pub async fn test_bi_request_and_can_not_send() {
        run_test(|| async {
            let test_service = mock_bi_service_echo();
            let request_stream = create_request_stream();

            let (giver, taker) = want::new();
            let (tx, _rx) = oneshot::channel::<Result<GrpcStream<Result<Payload, Error>>, Error>>();
            let callback = Callback::new(giver, tx);
            drop(taker);

            let ret = bi_request(Box::new(test_service), (request_stream, callback)).await;
            assert!(ret.is_err());
            assert_err_result(
                ret.unwrap_err(),
                "bi_request failed, callback can not invoke, receiver has been closed.",
            );
        })
        .await;
    }

    #[tokio::test]
    pub async fn test_bi_request_and_response_send_error() {
        run_test(|| async {
            let test_service = mock_bi_service_echo();
            let request_stream = create_request_stream();

            let (giver, mut taker) = want::new();
            let (tx, rx) = oneshot::channel::<Result<GrpcStream<Result<Payload, Error>>, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();
            drop(rx);

            let ret = bi_request(Box::new(test_service), (request_stream, callback)).await;
            assert!(ret.is_err());
            assert_err_result(
                ret.unwrap_err(),
                "bi_request failed, callback can not invoke, send error.",
            );
        })
        .await;
    }

    fn mock_bi_service_not_ready(msg: &str) -> MockBiCallTestService {
        let msg = msg.to_string();
        let mut test_service = MockBiCallTestService::new();
        test_service
            .expect_poll_ready()
            .returning(move |_| Poll::Ready(Err(Error::ErrResult(msg.clone()))));
        test_service.expect_call().returning(|request_stream| {
            let response_stream = stream! {
                for await request in request_stream {
                    yield Ok(request);
                }
            };
            let grpc_stream = GrpcStream::new(Box::pin(response_stream));
            Box::pin(async move { Ok(grpc_stream) })
        });
        test_service
    }

    #[tokio::test]
    pub async fn test_bi_request_and_request_service_is_ready() {
        run_test(|| async {
            let test_service = mock_bi_service_not_ready("test_service is not ready.");
            let request_stream = create_request_stream();

            let (giver, mut taker) = want::new();
            let (tx, _rx) = oneshot::channel::<Result<GrpcStream<Result<Payload, Error>>, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();

            let ret = bi_request(Box::new(test_service), (request_stream, callback)).await;
            assert!(ret.is_err());
            assert_err_result(ret.unwrap_err(), "test_service is not ready.");
        })
        .await;
    }

    fn create_request_stream_with_type(r#type: &str) -> GrpcStream<Payload> {
        let r#type = r#type.to_string();
        let request_stream = stream::once(async move {
            let mut payload = Payload::default();
            let mut metadata = Metadata::default();
            metadata.r#type = r#type;
            payload.metadata = Some(metadata);
            payload
        });
        GrpcStream::new(Box::pin(request_stream))
    }

    #[tokio::test]
    pub async fn test_bi_request() {
        run_test(|| async {
            let test_service = mock_bi_service_echo();
            let request_stream = create_request_stream_with_type("test_type");

            let (giver, mut taker) = want::new();
            let (tx, rx) = oneshot::channel::<Result<GrpcStream<Result<Payload, Error>>, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();

            let ret = bi_request(Box::new(test_service), (request_stream, callback)).await;
            assert!(ret.is_ok());

            let response = rx.await.unwrap().unwrap();
            let mut stream = Box::pin(response);
            let response = stream.next().await.unwrap().unwrap();

            assert_eq!(response.metadata.unwrap().r#type, "test_type");
        })
        .await;
    }

    fn mock_bi_service_error(msg: &str) -> MockBiCallTestService {
        let msg = msg.to_string();
        let mut test_service = MockBiCallTestService::new();
        test_service
            .expect_poll_ready()
            .returning(|_| Poll::Ready(Ok(())));
        test_service.expect_call().returning(move |request_stream| {
            let msg = msg.clone();
            let response_stream = stream! {
                for await _ in request_stream {
                    yield Err(Error::ErrResult(msg.clone()));
                }
            };
            let grpc_stream = GrpcStream::new(Box::pin(response_stream));
            Box::pin(async move { Ok(grpc_stream) })
        });
        test_service
    }

    #[tokio::test]
    pub async fn test_bi_request_and_response_error() {
        run_test(|| async {
            let test_service = mock_bi_service_error("test-error");
            let request_stream = create_request_stream();

            let (giver, mut taker) = want::new();
            let (tx, rx) = oneshot::channel::<Result<GrpcStream<Result<Payload, Error>>, Error>>();
            let callback = Callback::new(giver, tx);

            taker.want();

            let ret = bi_request(Box::new(test_service), (request_stream, callback)).await;
            assert!(ret.is_ok());

            let response = rx.await.unwrap().unwrap();
            let mut stream = Box::pin(response);
            assert_err_result(stream.next().await.unwrap().unwrap_err(), "test-error");
        })
        .await;
    }
}

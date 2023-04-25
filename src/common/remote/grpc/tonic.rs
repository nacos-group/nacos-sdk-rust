use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{Future, StreamExt};
use http::Uri;
use tonic::transport::{Channel, Endpoint};
use tower::{layer::util::Stack, Service};
use tracing::error;

use crate::{
    common::remote::grpc::nacos_grpc_service::DynamicBiStreamingCallLayerWrapper,
    nacos_proto::v2::{
        bi_request_stream_client::BiRequestStreamClient, request_client::RequestClient, Payload,
    },
};

use super::nacos_grpc_service::{
    BiStreamingCallIdentityLayer, DynamicBiStreamingCallLayer, DynamicBiStreamingCallService,
    DynamicUnaryCallLayer, DynamicUnaryCallLayerWrapper, DynamicUnaryCallService, GrpcStream,
    UnaryCallIdentityLayer,
};
use super::{
    config::GrpcConfiguration,
    nacos_grpc_service::{Callback, NacosGrpcCall},
};
use crate::api::error::Error;
use crate::api::error::Error::TonicGrpcStatus;

#[derive(Clone)]
pub(crate) struct Tonic {
    request_client: RequestClient<Channel>,
    bi_client: BiRequestStreamClient<Channel>,
    unary_call_layer: DynamicUnaryCallLayer,
    bi_call_layer: DynamicBiStreamingCallLayer,
}

impl Tonic {
    pub fn new(
        server: Uri,
        grpc_config: GrpcConfiguration,
        unary_call_layer: DynamicUnaryCallLayer,
        bi_call_layer: DynamicBiStreamingCallLayer,
    ) -> Self {
        let mut endpoint = Endpoint::from(server);

        if let Some(origin) = grpc_config.origin {
            endpoint = endpoint.origin(origin);
        }

        if let Some(user_agent) = grpc_config.user_agent {
            endpoint = endpoint.user_agent(user_agent).unwrap();
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

        let bi_client = BiRequestStreamClient::new(channel);

        Self {
            request_client,
            bi_client,
            unary_call_layer,
            bi_call_layer,
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
        let response = service.call(payload).await;
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
    };

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
        let response = service.call(stream).await;

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
    };

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
    S: Service<(), Response = Uri, Error = Error>,
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
    S: Service<(), Response = Uri, Error = Error>,
    S::Future: Send + 'static,
{
    type Response = Tonic;

    type Error = Error;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.server_list.poll_ready(cx)
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let server_info_fut = self.server_list.call(());
        let grpc_config = self.grpc_config.clone();
        let unary_call_layer = self.unary_call_layer.clone();
        let bi_call_layer = self.bi_call_layer.clone();

        let tonic_fut = async move {
            let server_info = server_info_fut.await?;
            let tonic = Tonic::new(server_info, grpc_config, unary_call_layer, bi_call_layer);
            Ok(tonic)
        };
        Box::pin(tonic_fut)
    }
}

impl Service<NacosGrpcCall> for Tonic {
    type Response = ();

    type Error = Error;

    type Future = GrpcCallTask;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, call: NacosGrpcCall) -> Self::Future {
        match call {
            NacosGrpcCall::RequestService(request) => {
                let unary_request_client = self.request_client.clone();
                let unary_call_service = UnaryCallService::new(unary_request_client);
                let dynamic_unary_call_service =
                    self.unary_call_layer.layer(Box::new(unary_call_service));
                unary_request(dynamic_unary_call_service, request)
            }
            NacosGrpcCall::BIRequestService(request) => {
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
}

impl GrpcCallTask {
    pub(crate) fn new(inner: InnerTask) -> Self {
        Self { inner }
    }
}

impl Future for GrpcCallTask {
    type Output = Result<(), Error>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let pin = unsafe { Pin::new_unchecked(self.get_mut().inner.as_mut()) };
        pin.poll(cx)
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
            let response = client.request(req).await;
            match response {
                Ok(ret) => Ok(ret.into_inner()),
                Err(status) => Err(TonicGrpcStatus(status)),
            }
        };
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
            let response = client.request_bi_stream(req).await;
            match response {
                Ok(response) => {
                    let response = response
                        .into_inner()
                        .map(|item| match item {
                            Ok(payload) => Ok(payload),
                            Err(status) => Err(TonicGrpcStatus(status)),
                        })
                        .boxed();
                    Ok(GrpcStream::new(response))
                }
                Err(status) => Err(TonicGrpcStatus(status)),
            }
        };
        Box::pin(fut)
    }
}

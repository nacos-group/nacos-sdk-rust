use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Future, Stream};
use tokio::sync::oneshot::Sender;
use tonic::async_trait;
use tower::{Layer, Service};
use want::Giver;

use crate::{api::error::Error, nacos_proto::v2::Payload};

pub(crate) enum NacosGrpcCall {
    RequestService((Payload, Callback<Result<Payload, Error>>)),

    BIRequestService(
        (
            GrpcStream<Payload>,
            Callback<Result<GrpcStream<Result<Payload, Error>>, Error>>,
        ),
    ),
}

pub(crate) struct GrpcStream<T> {
    inner: Pin<Box<dyn Stream<Item = T> + Send + 'static>>,
}

impl<T> GrpcStream<T> {
    pub(crate) fn new(inner: Pin<Box<dyn Stream<Item = T> + Send + 'static>>) -> Self {
        Self { inner }
    }
}

impl<T> Stream for GrpcStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pin = self.get_mut().inner.as_mut();
        pin.poll_next(cx)
    }
}

pub(crate) struct Callback<T> {
    gv: Giver,
    tx: Option<Sender<T>>,
}

impl<T> Callback<T> {
    pub(crate) fn new(gv: Giver, tx: Sender<T>) -> Self {
        Self { gv, tx: Some(tx) }
    }

    pub(crate) async fn can_send(&mut self) -> bool {
        if let Err(_closed) = self.gv.want().await {
            return false;
        }
        true
    }

    pub(crate) async fn send(&mut self, data: T) -> Result<(), Error> {
        let sender = self.tx.take();
        if let Some(sender) = sender {
            let _ = sender.send(data);
        }
        Ok(())
    }
}

#[async_trait]
pub(crate) trait ServerRequestHandler: Send + Sync {
    async fn request_reply(&self, request: Payload) -> Option<Payload>;
}

pub(crate) type DynamicUnaryCallService = Box<
    dyn Service<
            Payload,
            Error = Error,
            Response = Payload,
            Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send + 'static>>,
        > + Sync
        + Send,
>;
pub(crate) type DynamicUnaryCallLayer =
    Arc<dyn Layer<DynamicUnaryCallService, Service = DynamicUnaryCallService> + Sync + Send>;

pub(crate) struct DynamicUnaryCallLayerWrapper(pub(crate) DynamicUnaryCallLayer);

impl Layer<DynamicUnaryCallService> for DynamicUnaryCallLayerWrapper {
    type Service = DynamicUnaryCallService;

    fn layer(&self, inner: DynamicUnaryCallService) -> Self::Service {
        self.0.layer(inner)
    }
}

pub(crate) struct UnaryCallIdentityLayer;
impl Layer<DynamicUnaryCallService> for UnaryCallIdentityLayer {
    type Service = DynamicUnaryCallService;

    fn layer(&self, inner: DynamicUnaryCallService) -> Self::Service {
        inner
    }
}

pub(crate) type DynamicBiStreamingCallService = Box<
    dyn Service<
            GrpcStream<Payload>,
            Error = Error,
            Response = GrpcStream<Result<Payload, Error>>,
            Future = Pin<
                Box<
                    dyn Future<Output = Result<GrpcStream<Result<Payload, Error>>, Error>>
                        + Send
                        + 'static,
                >,
            >,
        > + Sync
        + Send
        + 'static,
>;
pub(crate) type DynamicBiStreamingCallLayer = Arc<
    dyn Layer<DynamicBiStreamingCallService, Service = DynamicBiStreamingCallService> + Sync + Send,
>;

pub(crate) struct DynamicBiStreamingCallLayerWrapper(pub(crate) DynamicBiStreamingCallLayer);

impl Layer<DynamicBiStreamingCallService> for DynamicBiStreamingCallLayerWrapper {
    type Service = DynamicBiStreamingCallService;

    fn layer(&self, inner: DynamicBiStreamingCallService) -> Self::Service {
        self.0.layer(inner)
    }
}

pub(crate) struct BiStreamingCallIdentityLayer;
impl Layer<DynamicBiStreamingCallService> for BiStreamingCallIdentityLayer {
    type Service = DynamicBiStreamingCallService;

    fn layer(&self, inner: DynamicBiStreamingCallService) -> Self::Service {
        inner
    }
}

#[cfg(test)]
pub mod unary_call_layer_test {
    use std::{pin::Pin, sync::Arc, task::Poll};

    use futures::{future::poll_fn, Future};
    use tower::{layer::util::Stack, Layer, Service};
    use tracing::{debug, metadata::LevelFilter};

    use crate::{
        api::error::Error,
        common::remote::grpc::nacos_grpc_service::DynamicUnaryCallLayerWrapper,
        nacos_proto::v2::{Metadata, Payload},
    };

    use super::{DynamicUnaryCallLayer, DynamicUnaryCallService, UnaryCallIdentityLayer};

    struct DynamicUnaryCallServiceA {
        inner: DynamicUnaryCallService,
    }

    impl Service<Payload> for DynamicUnaryCallServiceA {
        type Response = Payload;

        type Error = Error;

        type Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send + 'static>>;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            debug!("DynamicUnaryCallServiceA");
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, mut req: Payload) -> Self::Future {
            let mut metadata = req.metadata.take().unwrap();
            metadata
                .headers
                .insert("DynamicUnaryCallServiceA".to_string(), "ok".to_string());
            req.metadata = Some(metadata);
            self.inner.call(req)
        }
    }

    struct UnaryCallLayerA;

    impl Layer<DynamicUnaryCallService> for UnaryCallLayerA {
        type Service = DynamicUnaryCallService;

        fn layer(&self, inner: DynamicUnaryCallService) -> Self::Service {
            debug!("UnaryCallLayerA");
            Box::new(DynamicUnaryCallServiceA { inner })
        }
    }

    struct DynamicUnaryCallServiceB {
        inner: DynamicUnaryCallService,
    }

    impl Service<Payload> for DynamicUnaryCallServiceB {
        type Response = Payload;

        type Error = Error;

        type Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send + 'static>>;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            debug!("DynamicUnaryCallServiceB");
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, mut req: Payload) -> Self::Future {
            let mut metadata = req.metadata.take().unwrap();
            metadata
                .headers
                .insert("DynamicUnaryCallServiceB".to_string(), "ok".to_string());
            req.metadata = Some(metadata);
            self.inner.call(req)
        }
    }

    struct UnaryCallLayerB;

    impl Layer<DynamicUnaryCallService> for UnaryCallLayerB {
        type Service = DynamicUnaryCallService;

        fn layer(&self, inner: DynamicUnaryCallService) -> Self::Service {
            debug!("UnaryCallLayerB");
            Box::new(DynamicUnaryCallServiceB { inner })
        }
    }
    struct DynamicUnaryCallServiceC {
        inner: DynamicUnaryCallService,
    }

    impl Service<Payload> for DynamicUnaryCallServiceC {
        type Response = Payload;

        type Error = Error;

        type Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send + 'static>>;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            debug!("DynamicUnaryCallServiceC");
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, mut req: Payload) -> Self::Future {
            let mut metadata = req.metadata.take().unwrap();
            metadata
                .headers
                .insert("DynamicUnaryCallServiceC".to_string(), "ok".to_string());
            req.metadata = Some(metadata);
            self.inner.call(req)
        }
    }

    struct UnaryCallLayerC;

    impl Layer<DynamicUnaryCallService> for UnaryCallLayerC {
        type Service = DynamicUnaryCallService;

        fn layer(&self, inner: DynamicUnaryCallService) -> Self::Service {
            debug!("UnaryCallLayerC");
            Box::new(DynamicUnaryCallServiceC { inner })
        }
    }

    struct RealDynamicUnaryCallService;

    impl Service<Payload> for RealDynamicUnaryCallService {
        type Response = Payload;

        type Error = Error;

        type Future = Pin<Box<dyn Future<Output = Result<Payload, Error>> + Send + 'static>>;

        fn poll_ready(
            &mut self,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            debug!("RealDynamicUnaryCallService");
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, mut req: Payload) -> Self::Future {
            let mut metadata = req.metadata.take().unwrap();
            metadata
                .headers
                .insert("RealDynamicUnaryCallService".to_string(), "ok".to_string());
            req.metadata = Some(metadata);

            let fut = async move { Ok(req) };

            Box::pin(fut)
        }
    }

    struct UnaryCallLayerBuilder {
        layer: DynamicUnaryCallLayer,
    }

    impl UnaryCallLayerBuilder {
        fn new() -> Self {
            Self {
                layer: Arc::new(UnaryCallIdentityLayer),
            }
        }

        fn add_layer(self, layer: DynamicUnaryCallLayer) -> Self {
            let stack = Arc::new(Stack::new(
                DynamicUnaryCallLayerWrapper(layer),
                DynamicUnaryCallLayerWrapper(self.layer),
            ));
            Self {
                layer: stack,
                ..self
            }
        }

        fn build(self) -> DynamicUnaryCallLayer {
            self.layer
        }
    }

    #[ignore]
    #[tokio::test]
    pub async fn test() {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();

        let builder = UnaryCallLayerBuilder::new()
            .add_layer(Arc::new(UnaryCallLayerA))
            .add_layer(Arc::new(UnaryCallLayerB))
            .add_layer(Arc::new(UnaryCallLayerC));
        let layer = builder.build();

        let mut service = layer.layer(Box::new(RealDynamicUnaryCallService));

        let mut payload = Payload::default();

        let mut metadata = Metadata::default();
        metadata
            .headers
            .insert("init".to_string(), "ok".to_string());

        payload.metadata = Some(metadata);

        let ready = poll_fn(|cx| service.poll_ready(cx)).await;
        assert!(ready.is_ok());

        let ret = service.call(payload).await;

        assert!(ret.is_ok());

        let mut ret = ret.unwrap();
        let metadata = ret.metadata.take();

        assert!(metadata.is_some());

        let metadata = metadata.unwrap();

        let init = metadata.headers.get("init").unwrap();
        assert!(init.eq("ok"));

        let a = metadata.headers.get("DynamicUnaryCallServiceA").unwrap();
        assert!(a.eq("ok"));

        let b = metadata.headers.get("DynamicUnaryCallServiceB").unwrap();
        assert!(b.eq("ok"));

        let c = metadata.headers.get("DynamicUnaryCallServiceC").unwrap();
        assert!(c.eq("ok"));

        let d = metadata.headers.get("RealDynamicUnaryCallService").unwrap();
        assert!(d.eq("ok"));
    }
}

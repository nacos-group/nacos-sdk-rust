use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Future, Stream};

use async_trait::async_trait;
use tokio::sync::oneshot::Sender;
use tower::{Layer, Service};
use want::Giver;

use crate::api::error::Error::ErrResult;
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
        if let Some(sender) = sender
            && sender.send(data).is_err()
        {
            return Err(ErrResult(
                "callback send failed. the receiver has been dropped".to_string(),
            ));
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

#[cfg(test)]
mockall::mock! {
    pub(crate) DynamicUnaryCallService {}

    impl Service<Payload> for DynamicUnaryCallService {
        type Response = Payload;

        type Error = Error;

        type Future =
            Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready<'a>(&mut self, cx: &mut Context<'a>) -> Poll<Result<(), <Self as Service<Payload>>::Error>>;

        fn call(&mut self, req: Payload) -> <Self as Service<Payload>>::Future;
    }
}

pub(crate) type DynamicUnaryCallLayer =
    Arc<dyn Layer<DynamicUnaryCallService, Service = DynamicUnaryCallService> + Sync + Send>;

#[cfg(test)]
mockall::mock! {

    pub(crate) DynamicUnaryCallLayer{}

    impl Layer<MockDynamicUnaryCallService> for DynamicUnaryCallLayer {
        type Service = MockDynamicUnaryCallService;

        fn layer(&self, inner: MockDynamicUnaryCallService) -> <Self as Layer<MockDynamicUnaryCallService>>::Service;
    }


}

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

#[cfg(test)]
mockall::mock! {
    pub(crate) DynamicBiStreamingCallService {}

    impl Service<GrpcStream<Payload>> for DynamicBiStreamingCallService {
        type Response = GrpcStream<Result<Payload, Error>>;

        type Error = Error;

        type Future =
            Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready<'a>(&mut self, cx: &mut Context<'a>) -> Poll<Result<(), <Self as Service<GrpcStream<Payload>>>::Error>>;

        fn call(&mut self, req: GrpcStream<Payload>) -> <Self as Service<GrpcStream<Payload>>>::Future;

    }

}

pub(crate) type DynamicBiStreamingCallLayer = Arc<
    dyn Layer<DynamicBiStreamingCallService, Service = DynamicBiStreamingCallService> + Sync + Send,
>;

#[cfg(test)]
mockall::mock! {

    pub(crate) DynamicBiStreamingCallLayer {}

    impl Layer<MockDynamicBiStreamingCallService> for DynamicBiStreamingCallLayer {
        type Service = MockDynamicBiStreamingCallService;

        fn layer(&self, inner: MockDynamicBiStreamingCallService) -> <Self as Layer<MockDynamicBiStreamingCallService>>::Service;
    }

}

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

    use futures::{Future, future::poll_fn};
    use tower::{Layer, Service, layer::util::Stack};
    use tracing::debug;

    use crate::{
        api::error::Error,
        common::remote::grpc::nacos_grpc_service::DynamicUnaryCallLayerWrapper,
        nacos_proto::v2::{Metadata, Payload},
        test_config,
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
            let mut metadata = req
                .metadata
                .take()
                .expect("Metadata should be present in request");
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
            let mut metadata = req
                .metadata
                .take()
                .expect("Metadata should be present in request");
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
            let mut metadata = req
                .metadata
                .take()
                .expect("Metadata should be present in request");
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
            let mut metadata = req
                .metadata
                .take()
                .expect("Metadata should be present in request");
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

    #[tokio::test]
    pub async fn test() {
        run_test(|| async {
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

            let mut ret = ret.expect("Service call should have succeeded");
            let metadata = ret.metadata.take();

            assert!(metadata.is_some());

            let metadata = metadata.expect("Metadata should be present in response");

            let init = metadata
                .headers
                .get("init")
                .expect("init header should be present");
            assert!(init.eq("ok"));

            let a = metadata
                .headers
                .get("DynamicUnaryCallServiceA")
                .expect("DynamicUnaryCallServiceA header should be present");
            assert!(a.eq("ok"));

            let b = metadata
                .headers
                .get("DynamicUnaryCallServiceB")
                .expect("DynamicUnaryCallServiceB header should be present");
            assert!(b.eq("ok"));

            let c = metadata
                .headers
                .get("DynamicUnaryCallServiceC")
                .expect("DynamicUnaryCallServiceC header should be present");
            assert!(c.eq("ok"));

            let d = metadata
                .headers
                .get("RealDynamicUnaryCallService")
                .expect("RealDynamicUnaryCallService header should be present");
            assert!(d.eq("ok"));
        })
        .await;
    }
}

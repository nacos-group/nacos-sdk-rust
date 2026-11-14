use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;

use crate::nacos_proto::v2::Payload;

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
        self.get_mut().inner.as_mut().poll_next(cx)
    }
}

#[async_trait::async_trait]
pub(crate) trait ServerRequestHandler: Send + Sync {
    async fn request_reply(&self, request: Payload) -> Option<Payload>;
}

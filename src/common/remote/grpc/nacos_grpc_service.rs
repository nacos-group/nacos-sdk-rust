use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::oneshot::Sender;
use tonic::async_trait;
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

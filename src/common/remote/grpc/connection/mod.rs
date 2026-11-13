pub(crate) mod backoff;
pub(crate) mod config;
pub(crate) mod handshake;
pub(crate) mod stream;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Future;
use tokio::task::JoinHandle;

use crate::api::error::Error;
use crate::common::remote::grpc::nacos_grpc_service::{GrpcStream, ServerRequestHandler};
use crate::nacos_proto::v2::Payload;

pub(crate) use config::ConnectionSettings;

#[allow(dead_code)] // consumed by the connection supervisor in B4
pub(crate) type HandlerMap = HashMap<String, Arc<dyn ServerRequestHandler>>;

#[allow(dead_code)] // consumed by the connection supervisor in B4
#[derive(Debug)]
pub(crate) enum ConnectionEvent {
    StreamClosed(Error),
    Unhealthy(Error),
}

#[allow(dead_code)] // consumed by the connection supervisor in B4
#[async_trait]
pub(crate) trait Transport: Send + Sync + 'static {
    async fn unary(&self, payload: Payload) -> Result<Payload, Error>;

    async fn open_stream(
        &self,
        request: GrpcStream<Payload>,
    ) -> Result<GrpcStream<Result<Payload, Error>>, Error>;
}

#[allow(dead_code)] // consumed by the connection supervisor in B4
#[async_trait]
pub(crate) trait Connector: Send + 'static {
    async fn connect(&mut self) -> Result<Arc<dyn Transport>, Error>;
}

#[allow(dead_code)] // consumed by the connection supervisor in B4
pub(crate) trait Spawner: Send + Sync + 'static {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()>;
}

#[allow(dead_code)] // consumed by the connection supervisor in B4
pub(crate) struct ExecutorSpawner;

impl Spawner for ExecutorSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        crate::common::executor::spawn(future)
    }
}

#[allow(dead_code)] // consumed by the stream pump and health checks in B4
pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[allow(dead_code)] // consumed by the stream pump in B4
pub(crate) fn setup_ack_identity() -> &'static str {
    "SetupAckRequest"
}

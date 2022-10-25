use std::sync::Arc;

use futures::Future;
use tokio::sync::mpsc::Sender;

use crate::api::error::Result;
use crate::nacos_proto::v2::Payload;

mod default_handler;
mod naming_push_request_handler;

pub(crate) use default_handler::*;
pub(crate) use naming_push_request_handler::*;

pub(crate) type HandFutureType = Option<Box<dyn Future<Output = ()> + Send + Unpin + 'static>>;

pub trait GrpcPayloadHandler: Send + Sync + 'static {
    fn hand(&self, bi_sender: Arc<Sender<Result<Payload>>>, payload: Payload) -> HandFutureType;
}

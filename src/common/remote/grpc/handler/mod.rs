use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use crate::api::error::Result;
use crate::nacos_proto::v2::Payload;

pub(crate) mod default_handler;

pub(crate) trait GrpcPayloadHandler: Sync + Send + 'static {
    fn hand(&self, bi_sender: Arc<Sender<Result<Payload>>>, payload: Payload);
}

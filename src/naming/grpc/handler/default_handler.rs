use crate::nacos_proto::v2::Payload;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::info;

use super::GrpcPayloadHandler;

pub(crate) struct DefaultHandler;

impl GrpcPayloadHandler for DefaultHandler {
    fn hand(&self, _: Arc<Sender<Payload>>, payload: Payload) -> super::HandFutureType {
        let task = async move {
            info!("receive bi payload: {:?}", payload);
        };

        Some(Box::new(Box::pin(task)))
    }
}

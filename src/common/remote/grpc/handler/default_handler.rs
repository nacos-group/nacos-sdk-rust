use crate::nacos_proto::v2::Payload;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::info;

use super::GrpcPayloadHandler;
use crate::api::error::Result;

pub(crate) struct DefaultHandler;

impl GrpcPayloadHandler for DefaultHandler {
    fn hand(&self, _: Arc<Sender<Result<Payload>>>, payload: Payload) {
        info!("DefaultHandler receive a bi payload: {:?}", payload);
    }
}

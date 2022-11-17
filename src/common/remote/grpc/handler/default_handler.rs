use crate::{common::remote::grpc::bi_channel::ResponseWriter, nacos_proto::v2::Payload};
use tracing::info;

use super::GrpcPayloadHandler;

pub(crate) struct DefaultHandler;

impl GrpcPayloadHandler for DefaultHandler {
    fn hand(&self, _: ResponseWriter, payload: Payload) {
        info!("DefaultHandler receive a bi payload: {:?}", payload);
    }
}

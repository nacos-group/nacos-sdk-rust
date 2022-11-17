use crate::nacos_proto::v2::Payload;

use super::bi_channel::ResponseWriter;

pub(crate) mod client_detection_request_handler;
pub(crate) mod default_handler;

pub(crate) trait GrpcPayloadHandler: Sync + Send + 'static {
    fn hand(&self, response_writer: ResponseWriter, payload: Payload);
}

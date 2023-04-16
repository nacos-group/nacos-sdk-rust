use crate::{common::remote::grpc::bi_channel::ResponseWriter, nacos_proto::v2::Payload};
use tracing::info;

use super::GrpcPayloadHandler;

pub(crate) struct DefaultHandler;

impl GrpcPayloadHandler for DefaultHandler {
    fn hand(&self, _: ResponseWriter, payload: Payload) {
        let Payload { body, metadata } = payload;

        let body = body.unwrap_or_default();
        let p_type = body.type_url;
        let r_body = String::from_utf8(body.value).unwrap_or_default();

        let meta_data = metadata.unwrap_or_default();
        let r_type = meta_data.r#type;
        let client_ip = meta_data.client_ip;
        let headers = meta_data.headers;

        info!("unknown server request. type: {}, client_ip: {}, headers:{:?}, payload: {}, payload_type: {}", r_type, client_ip, headers, r_body, p_type);
    }
}

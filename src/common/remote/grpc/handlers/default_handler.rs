use crate::{
    common::remote::grpc::nacos_grpc_service::ServerRequestHandler, nacos_proto::v2::Payload,
};
use async_trait::async_trait;
use tracing::{error, info};

pub(crate) struct DefaultHandler;

#[async_trait]
impl ServerRequestHandler for DefaultHandler {
    async fn request_reply(&self, request: Payload) -> Option<Payload> {
        let p_type;
        let r_body;
        let r_type;
        let client_ip;
        let headers;

        if let Some(body) = request.body {
            p_type = body.type_url;
            let body_str = String::from_utf8(body.value);
            if let Err(e) = body_str {
                error!("unknown payload convert to string failed. {}", e);
                r_body = Default::default();
            } else {
                r_body = body_str.unwrap();
            }
        } else {
            r_body = Default::default();
            p_type = Default::default();
        }

        if let Some(meta_data) = request.metadata {
            r_type = meta_data.r#type;
            client_ip = meta_data.client_ip;
            headers = meta_data.headers;
        } else {
            r_type = Default::default();
            client_ip = Default::default();
            headers = Default::default();
        }

        info!(
            "unknown server request. type: {}, client_ip: {}, headers:{:?}, payload: {}, payload_type: {}",
            r_type, client_ip, headers, r_body, p_type
        );

        return None;
    }
}

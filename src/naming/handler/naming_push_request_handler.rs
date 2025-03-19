use std::sync::Arc;

use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::common::remote::grpc::nacos_grpc_service::ServerRequestHandler;
use crate::naming::observable::service_info_observable::ServiceInfoEmitter;

use async_trait::async_trait;
use tracing::{Instrument, error, info};

use crate::{
    nacos_proto::v2::Payload,
    naming::message::{request::NotifySubscriberRequest, response::NotifySubscriberResponse},
};

pub(crate) struct NamingPushRequestHandler {
    service_info_emitter: Arc<ServiceInfoEmitter>,
}

impl NamingPushRequestHandler {
    pub(crate) fn new(service_info_emitter: Arc<ServiceInfoEmitter>) -> Self {
        Self {
            service_info_emitter,
        }
    }
}

#[async_trait]
impl ServerRequestHandler for NamingPushRequestHandler {
    async fn request_reply(&self, request: Payload) -> Option<Payload> {
        let request = GrpcMessage::<NotifySubscriberRequest>::from_payload(request);
        if let Err(e) = request {
            error!("convert payload to NotifySubscriberRequest error. {e:?}");
            return None;
        }
        let request = request.unwrap();

        let body = request.into_body();
        info!("receive NotifySubscriberRequest from nacos server: {body:?}");

        let request_id = body.request_id;
        self.service_info_emitter
            .emit(body.service_info)
            .in_current_span()
            .await;

        let mut response = NotifySubscriberResponse::ok();
        response.request_id = request_id;

        let grpc_message = GrpcMessageBuilder::new(response).build();
        let payload = grpc_message.into_payload();
        if let Err(e) = payload {
            error!("occur an error when handing NotifySubscriberRequest. {e:?}");
            return None;
        }
        let payload = payload.unwrap();

        return Some(payload);
    }
}

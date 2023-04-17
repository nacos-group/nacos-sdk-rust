use crate::{
    common::remote::grpc::{
        message::{
            request::ClientDetectionRequest, response::ClientDetectionResponse, GrpcMessage,
            GrpcMessageBuilder,
        },
        nacos_grpc_service::ServerRequestHandler,
    },
    nacos_proto::v2::Payload,
};
use tonic::async_trait;
use tracing::{debug, debug_span, error};

pub(crate) struct ClientDetectionRequestHandler {
    pub(crate) client_id: String,
}

#[async_trait]
impl ServerRequestHandler for ClientDetectionRequestHandler {
    async fn request_reply(&self, request: Payload) -> Option<Payload> {
        let _client_detection_request_handler_span = debug_span!(
            parent: None,
            "client_detection_request_handler",
            client_id = self.client_id
        )
        .entered();

        let request_message = GrpcMessage::<ClientDetectionRequest>::from_payload(request);
        if let Err(e) = request_message {
            error!("convert payload to ClientDetectionRequest error. {e:?}");
            return None;
        }

        let request_message = request_message.unwrap();
        let request_message = request_message.into_body();
        debug!("ClientDetectionRequestHandler receive a request: {request_message:?}");
        let request_id = request_message.request_id;

        let mut response_message = ClientDetectionResponse::ok();
        response_message.request_id = request_id;

        let grpc_message = GrpcMessageBuilder::new(response_message).build();
        let payload = grpc_message.into_payload();
        if let Err(e) = payload {
            error!("occur an error when handing ClientDetectionRequest. {e:?}");
            return None;
        }
        let payload = payload.unwrap();
        Some(payload)
    }
}

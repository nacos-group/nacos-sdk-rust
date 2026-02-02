use crate::{
    common::remote::grpc::{
        message::{
            GrpcMessage, GrpcMessageBuilder, request::ClientDetectionRequest,
            response::ClientDetectionResponse,
        },
        nacos_grpc_service::ServerRequestHandler,
    },
    nacos_proto::v2::Payload,
};
use async_trait::async_trait;
use tracing::{debug, error};

pub(crate) struct ClientDetectionRequestHandler;

#[async_trait]
impl ServerRequestHandler for ClientDetectionRequestHandler {
    async fn request_reply(&self, request: Payload) -> Option<Payload> {
        let request_message = GrpcMessage::<ClientDetectionRequest>::from_payload(request);
        if let Err(e) = request_message {
            error!("convert payload to ClientDetectionRequest error. {e:?}");
            return None;
        }

        let request_message =
            request_message.expect("ClientDetectionRequest should be convertible from payload");
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
        let payload = payload.expect("Failed to convert ClientDetectionResponse to payload");
        Some(payload)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::nacos_proto::v2::Payload;

    #[tokio::test]
    pub async fn test_request_reply_when_convert_payload_error() {
        let request = Payload::default();
        let handler = ClientDetectionRequestHandler;
        let reply = handler.request_reply(request).await;

        assert!(reply.is_none())
    }

    #[tokio::test]
    pub async fn test_request_reply() {
        let mut request = ClientDetectionRequest::default();
        request.request_id = Some("test-request-id".to_string());
        let request_message = GrpcMessageBuilder::new(request).build();
        let payload = request_message
            .into_payload()
            .expect("Failed to convert request to payload");

        let handler = ClientDetectionRequestHandler;
        let reply = handler.request_reply(payload).await;

        assert!(reply.is_some());

        let reply = reply.expect("Reply should be present");

        let response = GrpcMessage::<ClientDetectionResponse>::from_payload(reply);

        assert!(response.is_ok());

        let response = response.expect("Response should be convertible from payload");

        let response = response.into_body();

        assert_eq!(
            response.request_id.expect("Request ID should be present"),
            "test-request-id".to_string()
        );
    }
}

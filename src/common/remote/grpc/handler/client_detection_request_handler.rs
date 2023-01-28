use crate::{
    common::{
        executor,
        remote::grpc::{
            bi_channel::ResponseWriter,
            message::{
                request::ClientDetectionRequest, response::ClientDetectionResponse, GrpcMessage,
                GrpcMessageBuilder,
            },
        },
    },
    nacos_proto::v2::Payload,
};
use tracing::{debug, error};

use super::GrpcPayloadHandler;

pub(crate) struct ClientDetectionRequestHandler;

impl GrpcPayloadHandler for ClientDetectionRequestHandler {
    fn hand(&self, response_writer: ResponseWriter, payload: Payload) {
        debug!(
            "ClientDetectionRequestHandler receive a bi payload: {:?}",
            payload
        );

        let request_message = GrpcMessage::<ClientDetectionRequest>::from_payload(payload);
        if let Err(e) = request_message {
            error!("convert payload to ClientDetectionRequest error. {:?}", e);
            return;
        }

        let request_message = request_message.unwrap();
        let request_message = request_message.into_body();
        let request_id = request_message.request_id;

        let mut response_message = ClientDetectionResponse::ok();
        response_message.request_id = request_id;

        let grpc_message = GrpcMessageBuilder::new(response_message).build();
        let payload = grpc_message.into_payload();
        if let Err(e) = payload {
            error!(
                "occur an error when handing ClientDetectionRequest. {:?}",
                e
            );
            return;
        }
        let payload = payload.unwrap();

        executor::spawn(async move {
            let ret = response_writer.write(payload).await;
            if let Err(e) = ret {
                error!(
                    "ClientDetectionRequestHandler write grpc message to server error. {:?}",
                    e
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use tracing::metadata::LevelFilter;

    use super::*;

    pub fn setup() {
        tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_max_level(LevelFilter::DEBUG)
            .init();
    }
    #[test]
    pub fn test_payload_convert_error() {
        setup();

        let target_handler = ClientDetectionRequestHandler;

        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::api::error::Result<Payload>>(1024);
        let response_writer = ResponseWriter::new(sender);

        let request = ClientDetectionRequest {
            request_id: Some("xd110022131".to_owned()),
            ..Default::default()
        };

        let request = GrpcMessageBuilder::new(request).build();
        let payload = request.into_payload().unwrap();

        target_handler.hand(response_writer, payload);

        let ret_payload = receiver.blocking_recv().unwrap().unwrap();

        let response_message = GrpcMessage::<ClientDetectionResponse>::from_payload(ret_payload);

        let response_message = response_message.unwrap();
        let response_message = response_message.into_body();

        let request_id = response_message.request_id.unwrap();

        assert_eq!(request_id, "xd110022131");
    }
}

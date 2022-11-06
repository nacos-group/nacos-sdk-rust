use crate::{
    common::{
        executor,
        remote::grpc::message::{
            request::ClientDetectionRequest, response::ClientDetectionResponse, GrpcMessage,
            GrpcMessageBuilder,
        },
    },
    nacos_proto::v2::Payload,
};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error};

use super::GrpcPayloadHandler;
use crate::api::error::Result;

pub(crate) struct ClientDetectionRequestHandler;

impl GrpcPayloadHandler for ClientDetectionRequestHandler {
    fn hand(&self, sender: Arc<Sender<Result<Payload>>>, payload: Payload) {
        debug!("GrpcPayloadHandler receive a bi payload: {:?}", payload);

        executor::spawn(async move {
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

            let ret = sender.send(Ok(payload)).await;
            if let Err(e) = ret {
                error!("ClientDetectionRequestHandler bi_sender send grpc message to server error. {:?}", e);
            }
        });
    }
}

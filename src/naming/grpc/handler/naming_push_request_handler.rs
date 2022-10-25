use super::GrpcPayloadHandler;
use std::sync::Arc;

use crate::api::error::Result;
use crate::api::events::naming::InstancesChangeEvent;
use tokio::sync::mpsc::Sender;
use tracing::error;

use crate::common::event_bus;
use crate::{
    common::executor,
    nacos_proto::v2::Payload,
    naming::grpc::message::{
        request::NotifySubscriberRequest, response::NotifySubscriberResponse, GrpcMessage,
        GrpcMessageBuilder,
    },
};

pub(crate) struct NamingPushRequestHandler {
    pub event_scope: String,
}

impl GrpcPayloadHandler for NamingPushRequestHandler {
    fn hand(
        &self,
        bi_sender: Arc<Sender<Result<Payload>>>,
        payload: Payload,
    ) -> super::HandFutureType {
        executor::spawn(async move {
            let response = NotifySubscriberResponse::ok();
            let grpc_message = GrpcMessageBuilder::new(response).build();
            let payload = grpc_message.into_payload();
            if let Err(e) = payload {
                error!(
                    "occur an error when handing NotifySubscriberRequest. {:?}",
                    e
                );
                return;
            }
            let payload = payload.unwrap();

            let ret = bi_sender.send(Ok(payload)).await;
            if let Err(e) = ret {
                error!("bi_sender send grpc message to server error. {:?}", e);
            }
        });
        let request = GrpcMessage::<NotifySubscriberRequest>::from_payload(payload);
        if let Err(e) = request {
            error!("convert payload to NotifySubscriberRequest error. {:?}", e);
            return None;
        }

        let request = request.unwrap();

        let body = request.into_body();

        let service_info = body.service_info;

        let event = InstancesChangeEvent {
            event_scope: self.event_scope.clone(),
            service_name: service_info.name,
            group_name: service_info.group_name,
            clusters: service_info.clusters,
            hosts: service_info.hosts,
        };

        Some(Box::new(Box::pin(async move {
            println!("NamingPushRequestHandler receive message: {:?}", event);
            event_bus::post(Box::new(event))
        })))
    }
}

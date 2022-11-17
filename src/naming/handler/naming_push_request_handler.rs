use std::sync::Arc;

use crate::api::events::naming::InstancesChangeEvent;
use crate::common::remote::grpc::bi_channel::ResponseWriter;
use crate::common::remote::grpc::handler::GrpcPayloadHandler;
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};

use tracing::{error, info};

use crate::common::event_bus;
use crate::{
    common::executor,
    nacos_proto::v2::Payload,
    naming::message::{request::NotifySubscriberRequest, response::NotifySubscriberResponse},
};

pub(crate) struct NamingPushRequestHandler {
    pub event_scope: String,
}

impl GrpcPayloadHandler for NamingPushRequestHandler {
    fn hand(&self, response_writer: ResponseWriter, payload: Payload) {
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

            let ret = response_writer.write(payload).await;
            if let Err(e) = ret {
                error!("bi_sender send grpc message to server error. {:?}", e);
            }
        });
        let request = GrpcMessage::<NotifySubscriberRequest>::from_payload(payload);
        if let Err(e) = request {
            error!("convert payload to NotifySubscriberRequest error. {:?}", e);
            return;
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

        executor::spawn(async move {
            info!("NamingPushRequestHandler receive message: {:?}", event);
            event_bus::post(Arc::new(event))
        });
    }
}

use crate::common::executor;
use crate::common::remote::grpc::bi_channel::ResponseWriter;
use crate::common::remote::grpc::handler::GrpcPayloadHandler;
use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::config::message::request::ConfigChangeNotifyRequest;
use crate::config::message::response::ConfigChangeNotifyResponse;
use crate::config::util;
use crate::nacos_proto::v2::Payload;
use tokio::sync::mpsc::Sender;

/// Handler for ConfigChangeNotify
pub(crate) struct ConfigChangeNotifyHandler {
    pub(crate) notify_change_tx: Sender<String>,
}

impl GrpcPayloadHandler for ConfigChangeNotifyHandler {
    fn hand(&self, response_writer: ResponseWriter, payload: Payload) {
        tracing::debug!("[ConfigChangeNotifyHandler] receive config-change, handle start.");

        let request = GrpcMessage::<ConfigChangeNotifyRequest>::from_payload(payload);
        if let Err(e) = request {
            tracing::error!(
                "convert payload to ConfigChangeNotifyRequest error. {:?}",
                e
            );
            return;
        }
        let server_req = request.unwrap().into_body();

        let notify_change_tx_clone = self.notify_change_tx.clone();

        executor::spawn(async move {
            let server_req_id = server_req.request_id.unwrap_or_else(|| "".to_string());
            let req_namespace = server_req.namespace.unwrap_or_else(|| "".to_string());
            let req_data_id = server_req.data_id.unwrap();
            let req_group = server_req.group.unwrap();
            tracing::info!(
                "receive config-change, dataId={},group={},namespace={}",
                req_data_id,
                req_group,
                req_namespace
            );
            // notify config change
            let group_key = util::group_key(&req_data_id, &req_group, &req_namespace);
            let _ = notify_change_tx_clone.send(group_key).await;

            // bi send resp
            let response = ConfigChangeNotifyResponse::ok().request_id(server_req_id);
            let grpc_message = GrpcMessageBuilder::new(response).build();
            let resp_payload = grpc_message.into_payload().unwrap();

            let ret = response_writer.write(resp_payload).await;
            if let Err(e) = ret {
                tracing::error!("bi_sender send grpc message to server error. {}", e);
            }
        });
    }
}

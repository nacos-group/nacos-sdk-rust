use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::common::remote::grpc::nacos_grpc_service::ServerRequestHandler;
use crate::config::message::request::ConfigChangeNotifyRequest;
use crate::config::message::response::ConfigChangeNotifyResponse;
use crate::config::util;
use crate::nacos_proto::v2::Payload;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

/// Handler for ConfigChangeNotify
pub(crate) struct ConfigChangeNotifyHandler {
    pub(crate) notify_change_tx: Sender<String>,
}

#[async_trait]
impl ServerRequestHandler for ConfigChangeNotifyHandler {
    async fn request_reply(&self, request: Payload) -> Option<Payload> {
        tracing::debug!("[ConfigChangeNotifyHandler] receive config-change, handle start.");

        let request = GrpcMessage::<ConfigChangeNotifyRequest>::from_payload(request);
        if let Err(e) = request {
            tracing::error!("convert payload to ConfigChangeNotifyRequest error. {e:?}");
            return None;
        }
        let server_req = request.expect("request should be valid").into_body();

        let server_req_id = server_req.request_id.unwrap_or_default();
        let req_namespace = server_req.namespace.unwrap_or_default();
        let req_data_id = server_req.data_id.expect("data_id should exist");
        let req_group = server_req.group.expect("group should exist");
        tracing::info!(
            "receive config-change, dataId={req_data_id},group={req_group},namespace={req_namespace}"
        );
        // notify config change
        let group_key = util::group_key(&req_data_id, &req_group, &req_namespace);
        let _ = self.notify_change_tx.send(group_key).await;

        // bi send resp
        let response = ConfigChangeNotifyResponse::ok().request_id(server_req_id);
        let grpc_message = GrpcMessageBuilder::new(response).build();
        let resp_payload = grpc_message
            .into_payload()
            .expect("payload conversion should succeed");

        Some(resp_payload)
    }
}

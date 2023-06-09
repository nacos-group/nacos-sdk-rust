use crate::common::remote::grpc::message::{GrpcMessage, GrpcMessageBuilder};
use crate::common::remote::grpc::nacos_grpc_service::ServerRequestHandler;
use crate::config::message::request::ConfigChangeNotifyRequest;
use crate::config::message::response::ConfigChangeNotifyResponse;
use crate::config::util;
use crate::nacos_proto::v2::Payload;
use tokio::sync::mpsc::Sender;
use tonic::async_trait;

/// Handler for ConfigChangeNotify
pub(crate) struct ConfigChangeNotifyHandler {
    pub(crate) notify_change_tx: Sender<String>,
    pub(crate) client_id: String,
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
        let server_req = request.unwrap().into_body();

        let server_req_id = server_req.request_id.unwrap_or_default();
        let req_namespace = server_req.namespace.unwrap_or_default();
        let req_data_id = server_req.data_id.unwrap();
        let req_group = server_req.group.unwrap();
        tracing::info!("receive config-change, dataId={req_data_id},group={req_group},namespace={req_namespace}");
        // notify config change
        let group_key = util::group_key(&req_data_id, &req_group, &req_namespace);
        let _ = self.notify_change_tx.send(group_key).await;

        // bi send resp
        let response = ConfigChangeNotifyResponse::ok().request_id(server_req_id);
        let grpc_message = GrpcMessageBuilder::new(response).build();
        let resp_payload = grpc_message.into_payload().unwrap();

        Some(resp_payload)
    }
}

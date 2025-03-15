use std::sync::Arc;

use async_trait::async_trait;
use tracing::Instrument;
use tracing::debug;
use tracing::error;
use tracing::instrument;

use crate::common::remote::generate_request_id;
use crate::common::remote::grpc::NacosGrpcClient;
use crate::common::remote::grpc::message::GrpcMessageData;
use crate::naming::message::request::SubscribeServiceRequest;
use crate::naming::message::response::SubscribeServiceResponse;
use crate::naming::redo::AutomaticRequest;
use crate::naming::redo::CallBack;

#[async_trait]
impl AutomaticRequest for SubscribeServiceRequest {
    #[instrument(skip_all)]
    async fn run(&self, grpc_client: Arc<NacosGrpcClient>, call_back: CallBack) {
        let mut request = self.clone();
        request.request_id = Some(generate_request_id());
        debug!("automatically execute subscribe service. {request:?}");
        let ret = grpc_client
            .send_request::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
            .in_current_span()
            .await;
        if let Err(e) = ret {
            error!("automatically execute subscribe service occur an error. {e:?}");
            call_back(Err(e));
        } else {
            call_back(Ok(()));
        }
    }

    fn name(&self) -> String {
        let namespace = self.namespace.as_deref().unwrap_or("");
        let service_name = self.service_name.as_deref().unwrap_or("");
        let group_name = self.group_name.as_deref().unwrap_or("");

        let request_name = format!(
            "{}@@{}@@{}@@{}",
            namespace,
            group_name,
            service_name,
            SubscribeServiceRequest::identity()
        );
        request_name
    }
}

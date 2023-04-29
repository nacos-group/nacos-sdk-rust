use std::sync::Arc;
use tonic::async_trait;
use tracing::debug;
use tracing::error;
use tracing::instrument;
use tracing::Instrument;

use crate::common::remote::grpc::NacosGrpcClient;
use crate::{
    common::remote::{generate_request_id, grpc::message::GrpcMessageData},
    naming::{
        message::{request::BatchInstanceRequest, response::BatchInstanceResponse},
        redo::AutomaticRequest,
    },
};

#[async_trait]
impl AutomaticRequest for BatchInstanceRequest {
    #[instrument(skip_all)]
    async fn run(
        &self,
        grpc_client: Arc<NacosGrpcClient>,
        call_back: crate::naming::redo::CallBack,
    ) {
        let mut request = self.clone();
        request.request_id = Some(generate_request_id());
        debug!("automatically execute batch instance request. {request:?}");
        let ret = grpc_client
            .send_request::<BatchInstanceRequest, BatchInstanceResponse>(request)
            .in_current_span()
            .await;
        if let Err(e) = ret {
            error!("automatically execute batch instance request occur an error. {e:?}");
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
            BatchInstanceRequest::identity()
        );
        request_name
    }
}

use tracing::info;

use crate::{
    common::{
        executor,
        remote::{generate_request_id, grpc::message::GrpcMessageData},
    },
    naming::{
        message::{request::BatchInstanceRequest, response::BatchInstanceResponse},
        redo::AutomaticRequest,
    },
};

impl AutomaticRequest for BatchInstanceRequest {
    fn run(
        &self,
        nacos_grpc_client: std::sync::Arc<crate::common::remote::grpc::NacosGrpcClient>,
        call_back: crate::naming::redo::CallBack,
    ) {
        let mut request = self.clone();
        request.request_id = Some(generate_request_id());

        executor::spawn(async move {
            let ret = nacos_grpc_client
                .unary_call_async::<BatchInstanceRequest, BatchInstanceResponse>(request)
                .await;
            if let Err(e) = ret {
                info!("batch instance automatic request error: {:?}", e);
                call_back(Err(e));
            } else {
                call_back(Ok(()));
            }
        });
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

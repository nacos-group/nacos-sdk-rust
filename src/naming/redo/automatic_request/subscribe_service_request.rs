use std::sync::Arc;

use crate::api::plugin::AuthPlugin;
use crate::common::executor;
use crate::common::remote::generate_request_id;
use crate::common::remote::grpc::message::GrpcMessageData;
use crate::common::remote::grpc::message::GrpcRequestMessage;
use crate::naming::message::request::SubscribeServiceRequest;
use crate::naming::message::response::SubscribeServiceResponse;
use crate::naming::redo::AutomaticRequest;
use crate::naming::redo::CallBack;

impl AutomaticRequest for SubscribeServiceRequest {
    fn run(
        &self,
        auth_plugin: Arc<dyn AuthPlugin>,
        nacos_grpc_client: std::sync::Arc<crate::common::remote::grpc::NacosGrpcClient>,
        call_back: CallBack,
    ) {
        let mut request = self.clone();
        request.request_id = Some(generate_request_id());
        request.add_headers(auth_plugin.get_login_identity().contexts);

        executor::spawn(async move {
            let ret = nacos_grpc_client
                .unary_call_async::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
                .await;
            if let Err(e) = ret {
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
            SubscribeServiceRequest::identity()
        );
        request_name
    }
}

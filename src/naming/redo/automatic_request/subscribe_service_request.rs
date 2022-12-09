use std::sync::Arc;

use tracing::debug;
use tracing::error;

use crate::common::executor;
use crate::common::remote::generate_request_id;
use crate::common::remote::grpc::message::GrpcMessageData;
use crate::naming::message::request::SubscribeServiceRequest;
use crate::naming::message::response::SubscribeServiceResponse;
use crate::naming::redo::AutomaticRequest;
use crate::naming::redo::AutomaticRequestInvoker;
use crate::naming::redo::CallBack;

impl AutomaticRequest for SubscribeServiceRequest {
    fn run(&self, invoker: Arc<AutomaticRequestInvoker>, call_back: CallBack) {
        let mut request = self.clone();
        request.request_id = Some(generate_request_id());
        debug!("automatically execute subscribe service. {:?}", request);
        executor::spawn(async move {
            let ret = invoker
                .invoke::<SubscribeServiceRequest, SubscribeServiceResponse>(request)
                .await;
            if let Err(e) = ret {
                error!(
                    "automatically execute subscribe service occur an error. {:?}",
                    e
                );
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

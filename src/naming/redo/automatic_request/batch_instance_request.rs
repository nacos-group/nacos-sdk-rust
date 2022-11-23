use std::sync::Arc;

use crate::naming::redo::AutomaticRequestInvoker;
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
    fn run(&self, invoker: Arc<AutomaticRequestInvoker>, call_back: crate::naming::redo::CallBack) {
        let mut request = self.clone();
        request.request_id = Some(generate_request_id());

        executor::spawn(async move {
            let ret = invoker
                .invoke::<BatchInstanceRequest, BatchInstanceResponse>(request)
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
            BatchInstanceRequest::identity()
        );
        request_name
    }
}

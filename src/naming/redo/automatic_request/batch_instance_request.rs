use std::sync::Arc;
use tracing::debug;
use tracing::error;
use tracing::Instrument;

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
        debug!("automatically execute batch instance request. {request:?}");
        executor::spawn(
            async move {
                let ret = invoker
                    .invoke::<BatchInstanceRequest, BatchInstanceResponse>(request)
                    .in_current_span()
                    .await;
                if let Err(e) = ret {
                    error!("automatically execute batch instance request occur an error. {e:?}");
                    call_back(Err(e));
                } else {
                    call_back(Ok(()));
                }
            }
            .in_current_span(),
        );
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

use std::sync::Arc;
use tracing::{Instrument, debug, error, instrument};

use crate::common::remote::generate_request_id;
use crate::common::remote::grpc::NacosGrpcClient;
use crate::common::remote::grpc::message::GrpcMessageData;
use crate::naming::message::request::*;
use crate::naming::message::response::*;
use crate::naming::redo::AutomaticRequest;
use crate::naming::redo::CallBack;

/// Macro to implement `AutomaticRequest` trait for redo tasks.
/// Reduces boilerplate code for request types that need automatic retry on connection failure.
// #[macro_export]
macro_rules! impl_automatic_request {
    ($request:ty, $response:ty) => {
        #[async_trait::async_trait]
        impl AutomaticRequest for $request {
            #[instrument(skip_all)]
            async fn run(&self, grpc_client: Arc<NacosGrpcClient>, call_back: CallBack) {
                let mut req = self.clone();
                req.request_id = Some(generate_request_id());
                debug!("automatically execute {}. {:?}", stringify!($request), req);
                let ret = grpc_client
                    .send_request::<$request, $response>(req)
                    .in_current_span()
                    .await;
                if let Err(e) = ret {
                    error!(
                        "automatically execute {} occur an error. {:?}",
                        stringify!($request),
                        e
                    );
                    call_back(Err(e));
                } else {
                    call_back(Ok(()));
                }
            }

            fn name(&self) -> String {
                let namespace = self.namespace.as_deref().unwrap_or("");
                let service_name = self.service_name.as_deref().unwrap_or("");
                let group_name = self.group_name.as_deref().unwrap_or("");
                format!(
                    "{}@@{}@@{}@@{}",
                    namespace,
                    group_name,
                    service_name,
                    <$request>::identity()
                )
            }
        }
    };
}

impl_automatic_request!(InstanceRequest, InstanceResponse);

impl_automatic_request!(BatchInstanceRequest, BatchInstanceResponse);

impl_automatic_request!(PersistentInstanceRequest, InstanceResponse);

impl_automatic_request!(SubscribeServiceRequest, SubscribeServiceResponse);

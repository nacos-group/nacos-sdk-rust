use std::sync::Arc;

use crate::api::error::Error::NamingRegisterServiceFailed;
use crate::api::error::Result;
use crate::api::naming::{NamingService, RegisterFuture, ServiceInstance};
use crate::api::props::ClientProps;

use crate::common::executor;
use crate::naming::grpc::message::request::InstanceRequest;
use crate::naming::grpc::message::response::InstanceResponse;
use crate::naming::grpc::{GrpcService, GrpcServiceBuilder};

use self::grpc::message::GrpcMessageBuilder;

mod grpc;

const LABEL_SOURCE: &str = "source";

const LABEL_SOURCE_SDK: &str = "sdk";

const LABEL_MODULE: &str = "module";

const LABEL_MODULE_NAMING: &str = "naming";

const DEFAULT_GROUP: &str = "DEFAULT_GROUP";

pub(crate) struct NacosNamingService {
    grpc_service: Arc<GrpcService>,
    client_props: ClientProps,
}

impl NacosNamingService {
    pub(crate) fn new(client_props: ClientProps) -> Self {
        let grpc_service = GrpcServiceBuilder::new()
            .address(client_props.server_addr.clone())
            .tenant(client_props.namespace.clone())
            .add_label(LABEL_SOURCE.to_owned(), LABEL_SOURCE_SDK.to_owned())
            .add_label(LABEL_MODULE.to_owned(), LABEL_MODULE_NAMING.to_owned())
            .build();
        let grpc_service = Arc::new(grpc_service);
        NacosNamingService {
            grpc_service,
            client_props,
        }
    }
}

const INSTANCE_REQUEST_REGISTER: &str = "registerInstance";
impl NamingService for NacosNamingService {
    fn register_service(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        let future = self.register_service_async(service_name, group_name, service_instance);
        executor::block_on(future)
    }

    fn register_service_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> RegisterFuture {
        let group_name = group_name.unwrap_or(DEFAULT_GROUP.to_string());
        let request = InstanceRequest::new(
            INSTANCE_REQUEST_REGISTER.to_string(),
            service_instance,
            self.client_props.namespace.clone(),
            service_name,
            group_name,
        );

        let request_headers = request.get_headers();
        let grpc_service = self.grpc_service.clone();
        let grpc_message = GrpcMessageBuilder::new(request)
            .headers(request_headers)
            .build();

        let task = async move {
            let ret = grpc_service
                .unary_call_async::<InstanceRequest, InstanceResponse>(grpc_message)
                .await;
            if ret.is_none() {
                return Err(NamingRegisterServiceFailed("empty response".to_string()));
            }
            let ret = ret.unwrap();
            let body = ret.body();
            if !body.is_success() {
                let message = body.message();
                if message.is_none() {
                    return Err(NamingRegisterServiceFailed(
                        "register failed and the server side didn't return any error message"
                            .to_string(),
                    ));
                }
                let message = message.unwrap();
                return Err(NamingRegisterServiceFailed(message));
            }

            return Ok(());
        };

        Box::new(Box::pin(task))
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use core::time;
    use std::thread;

    use tracing::info;

    use super::*;

    #[test]
    fn test_register_service() {
        let props = ClientProps::new()
            .server_addr("127.0.0.1:9848")
            .namespace("1c8beea4-810d-44c8-93cb-72997f71cb42")
            .app_name("rust-client");

        let naming_service = NacosNamingService::new(props);
        let service_instance = ServiceInstance::new("127.0.0.1".to_string(), 9090)
            .add_meta_data("netType".to_string(), "external".to_string())
            .add_meta_data("version".to_string(), "2.0".to_string());

        let collector = tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::with_default(collector, || {
            let ret = naming_service.register_service(
                "test-service".to_string(),
                Some(DEFAULT_GROUP.to_string()),
                service_instance,
            );
            info!("response. {:?}", ret);
        });

        let ten_millis = time::Duration::from_secs(10);
        thread::sleep(ten_millis);
    }
}

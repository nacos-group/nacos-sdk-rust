use std::collections::HashMap;
use std::sync::Arc;

use futures::Future;
use serde::{Deserialize, Serialize};

use crate::api::props::ClientProps;
use crate::naming::grpc::message::request::InstanceRequest;
use crate::naming::grpc::message::response::InstanceResponse;
use crate::naming::grpc::{GrpcService, GrpcServiceBuilder};

use self::grpc::message::{GrpcMessage, GrpcMessageBuilder};

mod error;
mod grpc;

const LABEL_SOURCE: &'static str = "source";

const LABEL_SOURCE_SDK: &'static str = "sdk";

const LABEL_MODULE: &'static str = "module";

const LABEL_MODULE_NAMING: &'static str = "naming";

const DEFAULT_CLUSTER_NAME: &'static str = "DEFAULT";

const DEFAULT_GROUP: &'static str = "DEFAULT_GROUP";

pub struct NamingService {
    grpc_service: Arc<GrpcService>,
    client_props: ClientProps,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceInstance {
    instance_id: Option<String>,

    ip: String,

    port: i32,

    weight: f64,

    healthy: bool,

    enabled: bool,

    ephemeral: bool,

    cluster_name: String,

    service_name: Option<String>,

    metadata: HashMap<String, String>,
}

impl ServiceInstance {
    pub fn new(ip: String, port: i32) -> Self {
        ServiceInstance {
            ip,
            port,
            instance_id: None,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: DEFAULT_CLUSTER_NAME.to_string(),
            service_name: None,
            metadata: HashMap::new(),
        }
    }
}

impl NamingService {
    pub fn new(client_props: ClientProps) -> Self {
        let grpc_service = GrpcServiceBuilder::new()
            .address(client_props.server_addr.clone())
            .tenant(client_props.namespace.clone())
            .add_label(LABEL_SOURCE.to_string(), LABEL_SOURCE_SDK.to_string())
            .add_label(LABEL_MODULE.to_string(), LABEL_MODULE_NAMING.to_string())
            .build();
        let grpc_service = Arc::new(grpc_service);
        NamingService {
            grpc_service,
            client_props,
        }
    }

    pub fn register_service(
        &self,
        service_name: String,
        group_name: String,
        service_instance: ServiceInstance,
    ) -> impl Future<Output = Option<GrpcMessage<InstanceResponse>>> {
        let request = InstanceRequest {
            r_type: "registerInstance".to_string(),
            namespace: self.client_props.namespace.clone(),
            service_name,
            group_mame: group_name,
            instance: service_instance,
        };
        let grpc_service = self.grpc_service.clone();
        let grpc_message = GrpcMessageBuilder::new(request).build();
        let task = async move {
            grpc_service
                .unary_call_async::<InstanceRequest, InstanceResponse>(grpc_message)
                .await
        };
        task
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use crate::common::executor;
    use tracing::info;

    use super::*;

    #[test]
    fn test_register_service() {
        let props = ClientProps::new()
            .server_addr("127.0.0.1:9848")
            .namespace("1c8beea4-810d-44c8-93cb-72997f71cb42")
            .app_name("rust-client");

        let naming_service = NamingService::new(props);
        let service_instance = ServiceInstance::new("127.0.0.1".to_string(), 9090);

        let collector = tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::with_default(collector, || {
            let taks = naming_service.register_service(
                "test-service".to_string(),
                DEFAULT_GROUP.to_string(),
                service_instance,
            );
            let ret = executor::block_on(taks);
            info!("response. {:?}", ret);
        });
    }
}

use std::sync::Arc;

use futures::Future;

use crate::api::error::Error::NamingBatchRegisterServiceFailed;
use crate::api::error::Error::NamingDeregisterServiceFailed;
use crate::api::error::Error::NamingRegisterServiceFailed;
use crate::api::error::Error::ServerNoResponse;
use crate::api::error::Result;
use crate::api::naming::{AsyncFuture, NamingService, ServiceInstance};
use crate::api::props::ClientProps;

use crate::common::executor;
use crate::naming::grpc::message::request::BatchInstanceRequest;
use crate::naming::grpc::message::request::InstanceRequest;
use crate::naming::grpc::message::response::BatchInstanceResponse;
use crate::naming::grpc::message::response::InstanceResponse;

use crate::naming::grpc::{GrpcService, GrpcServiceBuilder};

use self::grpc::message::GrpcMessageBuilder;
use self::grpc::message::GrpcRequestMessage;
use self::grpc::message::GrpcResponseMessage;

mod grpc;

pub(self) mod constants {

    pub const LABEL_SOURCE: &str = "source";

    pub const LABEL_SOURCE_SDK: &str = "sdk";

    pub const LABEL_MODULE: &str = "module";

    pub const LABEL_MODULE_NAMING: &str = "naming";

    pub const DEFAULT_GROUP: &str = "DEFAULT_GROUP";

    pub const DEFAULT_TENANT: &str = "public";

    pub const APP_FILED: &str = "app";

    pub const DEFAULT_APP_NAME: &str = "unknown";

    pub mod request {
        pub const INSTANCE_REQUEST_REGISTER: &str = "registerInstance";
        pub const DE_REGISTER_INSTANCE: &str = "deregisterInstance";
        pub const BATCH_REGISTER_INSTANCE: &str = "batchRegisterInstance";
    }
}

pub(crate) struct NacosNamingService {
    grpc_service: Arc<GrpcService>,
    namespace: String,
    app_name: String,
}

impl NacosNamingService {
    pub(crate) fn new(client_props: ClientProps) -> Self {
        let app_name = client_props
            .app_name
            .unwrap_or_else(|| self::constants::DEFAULT_APP_NAME.to_owned());
        let mut tenant = client_props.namespace;
        if tenant.is_empty() {
            tenant = self::constants::DEFAULT_TENANT.to_owned();
        }

        let grpc_service = GrpcServiceBuilder::new()
            .address(client_props.server_addr)
            .tenant(tenant.clone())
            .client_version(client_props.client_version)
            .support_remote_connection(true)
            .support_remote_metrics(true)
            .support_delta_push(false)
            .support_remote_metric(false)
            .add_label(
                self::constants::LABEL_SOURCE.to_owned(),
                self::constants::LABEL_SOURCE_SDK.to_owned(),
            )
            .add_label(
                self::constants::LABEL_MODULE.to_owned(),
                self::constants::LABEL_MODULE_NAMING.to_owned(),
            )
            .add_labels(client_props.labels)
            .build();
        let grpc_service = Arc::new(grpc_service);
        NacosNamingService {
            grpc_service,
            namespace: tenant,
            app_name,
        }
    }

    fn request_to_server<R, P>(
        &self,
        mut request: R,
    ) -> Box<dyn Future<Output = Result<P>> + Send + Unpin + 'static>
    where
        R: GrpcRequestMessage + 'static,
        P: GrpcResponseMessage + 'static,
    {
        let request_headers = request.take_headers();
        let grpc_service = self.grpc_service.clone();

        let grpc_message = GrpcMessageBuilder::new(request)
            .header(self::constants::APP_FILED.to_owned(), self.app_name.clone())
            .headers(request_headers)
            .build();

        let task = async move {
            let ret = grpc_service.unary_call_async::<R, P>(grpc_message).await;
            if ret.is_none() {
                return Err(ServerNoResponse);
            }
            let ret = ret.unwrap();
            let body = ret.into_body();
            Ok(body)
        };

        Box::new(Box::pin(task))
    }

    fn instance_opt(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
        r_type: String,
    ) -> Box<dyn Future<Output = Result<InstanceResponse>> + Send + Unpin + 'static> {
        let group_name = group_name.unwrap_or_else(|| self::constants::DEFAULT_GROUP.to_owned());
        let request = InstanceRequest {
            r_type,
            instance: service_instance,
            namespace: self.namespace.clone(),
            service_name,
            group_name,
            ..Default::default()
        };

        let reqeust_to_server_task =
            self.request_to_server::<InstanceRequest, InstanceResponse>(request);
        Box::new(Box::pin(async move { reqeust_to_server_task.await }))
    }

    fn batch_instances_opt(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
        r_type: String,
    ) -> Box<dyn Future<Output = Result<BatchInstanceResponse>> + Send + Unpin + 'static> {
        let group_name = group_name.unwrap_or_else(|| self::constants::DEFAULT_GROUP.to_owned());
        let request = BatchInstanceRequest {
            r_type,
            instances: service_instances,
            namespace: self.namespace.clone(),
            service_name,
            group_name,
            ..Default::default()
        };

        let reqeust_to_server_task =
            self.request_to_server::<BatchInstanceRequest, BatchInstanceResponse>(request);
        Box::new(Box::pin(async move { reqeust_to_server_task.await }))
    }
}

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
    ) -> AsyncFuture {
        let instance_opt_task = self.instance_opt(
            service_name,
            group_name,
            service_instance,
            self::constants::request::INSTANCE_REQUEST_REGISTER.to_owned(),
        );

        Box::new(Box::pin(async move {
            let response = instance_opt_task.await;

            let body = response?;
            if !body.is_success() {
                let InstanceResponse {
                    result_code,
                    error_code,
                    message,
                    ..
                } = body;
                return Err(NamingRegisterServiceFailed(
                    result_code,
                    error_code,
                    message.unwrap_or_default(),
                ));
            }

            Ok(())
        }))
    }

    fn deregister_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> Result<()> {
        let future = self.deregister_instance_async(service_name, group_name, service_instance);
        executor::block_on(future)
    }

    fn deregister_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instance: ServiceInstance,
    ) -> AsyncFuture {
        let instance_opt_task = self.instance_opt(
            service_name,
            group_name,
            service_instance,
            self::constants::request::DE_REGISTER_INSTANCE.to_owned(),
        );

        Box::new(Box::pin(async move {
            let response = instance_opt_task.await;

            let body = response?;
            if !body.is_success() {
                let InstanceResponse {
                    result_code,
                    error_code,
                    message,
                    ..
                } = body;
                return Err(NamingDeregisterServiceFailed(
                    result_code,
                    error_code,
                    message.unwrap_or_default(),
                ));
            }

            Ok(())
        }))
    }

    fn batch_register_instance(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> Result<()> {
        let future =
            self.batch_register_instance_async(service_name, group_name, service_instances);
        executor::block_on(future)
    }

    fn batch_register_instance_async(
        &self,
        service_name: String,
        group_name: Option<String>,
        service_instances: Vec<ServiceInstance>,
    ) -> AsyncFuture {
        let batch_instance_opt_task = self.batch_instances_opt(
            service_name,
            group_name,
            service_instances,
            self::constants::request::BATCH_REGISTER_INSTANCE.to_owned(),
        );

        Box::new(Box::pin(async move {
            let response = batch_instance_opt_task.await;

            let body = response?;
            if !body.is_success() {
                let BatchInstanceResponse {
                    result_code,
                    error_code,
                    message,
                    ..
                } = body;
                return Err(NamingBatchRegisterServiceFailed(
                    result_code,
                    error_code,
                    message.unwrap_or_default(),
                ));
            }

            Ok(())
        }))
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
        let props = ClientProps::new().server_addr("127.0.0.1:9848");

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
                Some(constants::DEFAULT_GROUP.to_string()),
                service_instance,
            );
            info!("response. {:?}", ret);
        });

        let ten_millis = time::Duration::from_secs(100);
        thread::sleep(ten_millis);
    }

    #[test]
    fn test_register_and_deregister_service() {
        let props = ClientProps::new().server_addr("127.0.0.1:9848");

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
                Some(constants::DEFAULT_GROUP.to_string()),
                service_instance.clone(),
            );
            info!("response. {:?}", ret);

            let ten_millis = time::Duration::from_secs(10);
            thread::sleep(ten_millis);

            let ret = naming_service.deregister_instance(
                "test-service".to_string(),
                Some(constants::DEFAULT_GROUP.to_string()),
                service_instance,
            );
            info!("response. {:?}", ret);

            let ten_millis = time::Duration::from_secs(10);
            thread::sleep(ten_millis);
        });
    }

    #[test]
    fn test_batch_register_service() {
        let props = ClientProps::new().server_addr("127.0.0.1:9848");

        let naming_service = NacosNamingService::new(props);
        let service_instance1 = ServiceInstance::new("127.0.0.1".to_string(), 9090)
            .add_meta_data("netType".to_string(), "external".to_string())
            .add_meta_data("version".to_string(), "2.0".to_string());

        let service_instance2 = ServiceInstance::new("192.168.1.1".to_string(), 8080)
            .add_meta_data("netType".to_string(), "external".to_string())
            .add_meta_data("version".to_string(), "2.0".to_string());

        let service_instance3 = ServiceInstance::new("192.168.1.2".to_string(), 8081)
            .add_meta_data("netType".to_string(), "external".to_string())
            .add_meta_data("version".to_string(), "2.0".to_string());

        let instance_vec = vec![service_instance1, service_instance2, service_instance3];

        let collector = tracing_subscriber::fmt()
            .with_thread_names(true)
            .with_file(true)
            .with_level(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .finish();

        tracing::subscriber::with_default(collector, || {
            let ret = naming_service.batch_register_instance(
                "test-service".to_string(),
                Some(constants::DEFAULT_GROUP.to_string()),
                instance_vec,
            );
            info!("response. {:?}", ret);

            let ten_millis = time::Duration::from_secs(10);
            thread::sleep(ten_millis);
        });
    }
}

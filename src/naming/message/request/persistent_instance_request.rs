use nacos_macro::request;

use crate::{api::naming::ServiceInstance, common::remote::generate_request_id};

#[request(identity = "PersistentInstanceRequest", module = "naming")]
pub(crate) struct PersistentInstanceRequest {
    #[serde(rename = "type")]
    pub(crate) r_type: String,
    pub(crate) instance: ServiceInstance,
}

impl PersistentInstanceRequest {
    pub(crate) fn register(
        instance: ServiceInstance,
        service_name: Option<String>,
        namespace: Option<String>,
        group_name: Option<String>,
    ) -> Self {
        PersistentInstanceRequest::new(
            RequestType::Register,
            instance,
            service_name,
            namespace,
            group_name,
        )
    }

    pub(crate) fn deregister(
        instance: ServiceInstance,
        service_name: Option<String>,
        namespace: Option<String>,
        group_name: Option<String>,
    ) -> Self {
        PersistentInstanceRequest::new(
            RequestType::Deregister,
            instance,
            service_name,
            namespace,
            group_name,
        )
    }

    fn new(
        request_type: RequestType,
        instance: ServiceInstance,
        service_name: Option<String>,
        namespace: Option<String>,
        group_name: Option<String>,
    ) -> Self {
        let request_id = Some(generate_request_id());
        Self {
            r_type: request_type.request_code(),
            instance,
            request_id,
            namespace,
            service_name,
            group_name,
            ..Default::default()
        }
    }
}

pub(crate) enum RequestType {
    Register,
    Deregister,
}

impl RequestType {
    pub(crate) fn request_code(&self) -> String {
        match self {
            RequestType::Register => "registerInstance".to_string(),
            RequestType::Deregister => "deregisterInstance".to_string(),
        }
    }
}

impl From<&str> for RequestType {
    fn from(code: &str) -> Self {
        match code {
            "registerInstance" => RequestType::Register,
            "deregisterInstance" => RequestType::Deregister,
            _ => RequestType::Register,
        }
    }
}

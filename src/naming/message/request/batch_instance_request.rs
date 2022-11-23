use nacos_macro::request;

use crate::{api::naming::ServiceInstance, common::remote::generate_request_id};

#[request(identity = "BatchInstanceRequest", module = "naming")]
pub(crate) struct BatchInstanceRequest {
    #[serde(rename = "type")]
    pub(crate) r_type: String,

    pub(crate) instances: Vec<ServiceInstance>,
}

impl BatchInstanceRequest {
    pub(crate) fn new(
        instances: Vec<ServiceInstance>,
        namespace: Option<String>,
        service_name: Option<String>,
        group_name: Option<String>,
    ) -> Self {
        let request_id = Some(generate_request_id());
        Self {
            r_type: "batchRegisterInstance".to_string(),
            instances,
            request_id,
            namespace,
            service_name,
            group_name,
            ..Default::default()
        }
    }
}

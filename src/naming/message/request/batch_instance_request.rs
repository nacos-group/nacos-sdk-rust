use nacos_macro::request;

use crate::api::naming::ServiceInstance;

#[request(identity = "BatchInstanceRequest", module = "naming")]
pub(crate) struct BatchInstanceRequest {
    #[serde(rename = "type")]
    pub(crate) r_type: String,

    pub(crate) instances: Vec<ServiceInstance>,
}

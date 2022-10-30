use nacos_macro::request;

use crate::naming::ServiceInstance;

#[request(identity = "InstanceRequest", module = "naming")]
pub(crate) struct InstanceRequest {
    #[serde(rename = "type")]
    pub(crate) r_type: String,
    pub(crate) instance: ServiceInstance,
}

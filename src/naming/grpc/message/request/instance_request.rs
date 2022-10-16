use nacos_macro::request;

use crate::naming::ServiceInstance;

#[request(identity = "InstanceRequest", module = "naming")]
pub(crate) struct InstanceRequest {
    #[serde(rename = "type")]
    pub r_type: String,
    pub instance: ServiceInstance,
}

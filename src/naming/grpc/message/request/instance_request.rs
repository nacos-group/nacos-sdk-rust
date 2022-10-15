use nacos_macro::request;

use crate::naming::ServiceInstance;

#[request(identity = "InstanceRequest")]
pub(crate) struct InstanceRequest {
    #[serde(rename = "type")]
    pub r_type: String,
    pub instance: ServiceInstance,
    pub namespace: String,
    pub service_name: String,
    pub group_name: String,
}

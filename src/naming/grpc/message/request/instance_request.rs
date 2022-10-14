use nacos_macro::request;

use crate::naming::ServiceInstance;

#[request(identity = "InstanceRequest")]
pub(crate) struct InstanceRequest {
    #[serde(rename = "type")]
    r_type: String,
    instance: ServiceInstance,
    namespace: String,
    service_name: String,

    group_name: String,
}

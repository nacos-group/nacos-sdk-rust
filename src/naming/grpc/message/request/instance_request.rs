use crate::naming::ServiceInstance;

use crate::naming::grpc::message::GrpcMessageBody;
use nacos_macro::GrpcMessageBody;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
#[message_attr(request_type = "InstanceRequest")]
pub(crate) struct InstanceRequest {
    #[serde(rename = "type")]
    pub(crate) r_type: String,

    pub(crate) instance: ServiceInstance,

    pub(crate) namespace: String,

    #[serde(rename = "serviceName")]
    pub(crate) service_name: String,

    #[serde(rename = "groupName")]
    pub(crate) group_mame: String,
}

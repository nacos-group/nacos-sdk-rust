use crate::naming::grpc::message::GrpcMessageBody;
use nacos_macro::GrpcMessageBody;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
#[message_attr(request_type = "InstanceResponse")]
pub struct InstanceResponse {
    #[serde(rename = "type")]
    pub r_type: Option<String>,

    #[serde(rename = "resultCode")]
    pub result_code: i32,

    #[serde(rename = "errorCode")]
    pub error_code: i32,

    pub message: Option<String>,

    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
}

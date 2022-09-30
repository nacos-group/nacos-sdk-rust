use crate::naming::grpc::message::GrpcMessageBody;
use nacos_macro::GrpcMessageBody;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
#[message_attr(request_type = "ErrorResponse")]
pub struct ErrorResponse {
    #[serde(rename = "resultCode")]
    pub result_code: i32,

    #[serde(rename = "errorCode")]
    pub error_code: i32,

    #[serde(rename = "message")]
    pub message: String,
}

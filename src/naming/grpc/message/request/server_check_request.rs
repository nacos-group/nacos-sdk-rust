use crate::naming::grpc::message::GrpcMessageBody;
use nacos_macro::GrpcMessageBody;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
#[message_attr(request_type = "ServerCheckRequest")]
pub struct ServerCheckRequest {}

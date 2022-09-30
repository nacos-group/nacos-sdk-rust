use std::collections::HashMap;

use crate::naming::grpc::{client_abilities::ClientAbilities, message::GrpcMessageBody};
use nacos_macro::GrpcMessageBody;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, GrpcMessageBody)]
#[message_attr(request_type = "ConnectionSetupRequest")]
pub(crate) struct ConnectionSetupRequest {
    #[serde(rename = "clientVersion")]
    client_version: String,

    abilities: ClientAbilities,

    tenant: String,

    labels: HashMap<String, String>,
}

impl ConnectionSetupRequest {
    pub(crate) fn new(
        client_version: String,
        abilities: ClientAbilities,
        tenant: String,
        labels: HashMap<String, String>,
    ) -> Self {
        ConnectionSetupRequest {
            client_version,
            labels,
            abilities,
            tenant,
        }
    }
}

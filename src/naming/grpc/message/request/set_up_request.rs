use std::collections::HashMap;

use nacos_macro::request;

use crate::naming::grpc::client_abilities::ClientAbilities;

#[request(identity = "ConnectionSetupRequest")]
pub(crate) struct ConnectionSetupRequest {
    client_version: String,

    abilities: ClientAbilities,

    tenant: String,

    labels: HashMap<String, String>,
}

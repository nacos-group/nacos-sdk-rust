use std::collections::HashMap;

use nacos_macro::request;

use crate::naming::grpc::client_abilities::ClientAbilities;

#[request(identity = "ConnectionSetupRequest", module = "naming")]
pub(crate) struct ConnectionSetupRequest {
    pub client_version: String,

    pub abilities: ClientAbilities,

    pub tenant: String,

    pub labels: HashMap<String, String>,
}

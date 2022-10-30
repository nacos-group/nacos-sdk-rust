use std::collections::HashMap;

use nacos_macro::request;

use crate::common::remote::grpc::NacosClientAbilities;

#[request(identity = "ConnectionSetupRequest", module = "internal")]
pub(crate) struct ConnectionSetupRequest {
    pub(crate) client_version: String,

    pub(crate) abilities: NacosClientAbilities,

    pub(crate) tenant: String,

    pub(crate) labels: HashMap<String, String>,
}

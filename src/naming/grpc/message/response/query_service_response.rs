use nacos_macro::response;
use serde::{Deserialize, Serialize};

use crate::api::naming::ServiceInstance;

#[response(identity = "QueryServiceResponse", module = "naming")]
pub struct QueryServiceResponse {
    pub service_info: ServiceInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,

    pub group_name: String,

    pub clusters: String,

    pub cache_millis: i64,

    pub last_ref_time: i64,

    pub checksum: String,

    #[serde(rename = "allIPs")]
    pub all_ips: bool,

    pub reach_protection_threshold: bool,

    pub hosts: Vec<ServiceInstance>,
}

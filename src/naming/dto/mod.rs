use serde::{Deserialize, Serialize};

use crate::api::naming::ServiceInstance;

use crate::api::error::Error::GroupNameParseErr;
use crate::api::error::Result;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

const SERVICE_INFO_SEPARATOR: &str = "@@";
impl ServiceInfo {
    pub fn new(key: String) -> Result<Self> {
        let max_index = 2;
        let cluster_index = 2;
        let service_name_index = 1;
        let group_index = 0;

        let keys: Vec<_> = key.split(SERVICE_INFO_SEPARATOR).collect();

        if key.len() > max_index {
            Ok(ServiceInfo {
                group_name: keys[group_index].to_owned(),
                name: keys[service_name_index].to_owned(),
                clusters: keys[cluster_index].to_owned(),
                ..Default::default()
            })
        } else if keys.len() == max_index {
            Ok(ServiceInfo {
                group_name: keys[group_index].to_owned(),
                name: keys[service_name_index].to_owned(),
                ..Default::default()
            })
        } else {
            Err(GroupNameParseErr(
                "group name must not be null!".to_string(),
            ))
        }
    }
}

impl Default for ServiceInfo {
    fn default() -> Self {
        Self {
            name: Default::default(),
            group_name: Default::default(),
            clusters: Default::default(),
            cache_millis: 1000,
            last_ref_time: 0,
            checksum: Default::default(),
            all_ips: false,
            reach_protection_threshold: false,
            hosts: Default::default(),
        }
    }
}

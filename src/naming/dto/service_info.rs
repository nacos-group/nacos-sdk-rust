use serde::{Deserialize, Serialize};
use tracing::error;

use crate::api::naming::ServiceInstance;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,

    pub group_name: String,

    pub clusters: String,

    pub cache_millis: i64,

    pub last_ref_time: i64,

    pub checksum: String,

    #[serde(rename = "allIps")]
    pub all_ips: bool,

    pub reach_protection_threshold: bool,

    pub hosts: Option<Vec<ServiceInstance>>,
}

const SERVICE_INFO_SEPARATOR: &str = "@@";
impl ServiceInfo {
    pub fn ip_count(&self) -> i32 {
        if self.hosts.is_none() {
            return 0;
        }
        self.hosts.as_ref().unwrap().len() as i32
    }

    pub fn validate(&self) -> bool {
        if self.all_ips {
            return true;
        }

        if self.hosts.is_none() {
            return false;
        }

        let hosts = self.hosts.as_ref().unwrap();
        for host in hosts {
            if !host.healthy {
                continue;
            }

            if host.weight > 0 as f64 {
                return true;
            }
        }

        false
    }

    pub fn get_grouped_service_name(service_name: &str, group_name: &str) -> String {
        if !group_name.is_empty() && !service_name.contains(SERVICE_INFO_SEPARATOR) {
            let service_name = format!("{}{}{}", &group_name, SERVICE_INFO_SEPARATOR, service_name);
            return service_name;
        }
        service_name.to_string()
    }

    pub fn hosts_to_json(&self) -> String {
        if self.hosts.is_none() {
            return "".to_string();
        }
        let json = serde_json::to_string(self.hosts.as_ref().unwrap());
        if let Err(e) = json {
            error!("hosts to json failed. {e:?}");
            return "".to_string();
        }
        json.unwrap()
    }

    pub fn get_key(name: &str, clusters: &str) -> String {
        if !clusters.is_empty() {
            let key = format!("{}{}{}", name, SERVICE_INFO_SEPARATOR, clusters);
            return key;
        }

        name.to_string()
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

use std::sync::Arc;

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

    // < 3.0
    #[serde(rename = "allIPs")]
    pub all_ips: Option<bool>,

    // >=3.0
    #[serde(rename = "allIps")]
    pub all_ips_3x: Option<bool>,

    pub reach_protection_threshold: bool,

    pub hosts: Option<Arc<Vec<ServiceInstance>>>,
}

const SERVICE_INFO_SEPARATOR: &str = "@@";
impl ServiceInfo {
    pub fn ip_count(&self) -> i32 {
        match self.hosts.as_ref() {
            Some(h) => h.len() as i32,
            None => 0,
        }
    }

    fn is_all_ips(&self) -> bool {
        if let Some(item) = self.all_ips {
            item
        } else {
            self.all_ips_3x.unwrap_or(false)
        }
    }

    pub fn validate(&self) -> bool {
        if self.is_all_ips() {
            return true;
        }

        let Some(hosts) = self.hosts.as_ref() else {
            return false;
        };

        for host in hosts.iter() {
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
            let service_name = format!("{}{}{}", group_name, SERVICE_INFO_SEPARATOR, service_name);
            return service_name;
        }
        service_name.to_string()
    }

    pub fn hosts_to_json(&self) -> String {
        let Some(hosts) = self.hosts.as_ref() else {
            return "".to_owned();
        };
        serde_json::to_string(hosts).unwrap_or_else(|e| {
            error!("hosts to json failed. {e:?}");
            "".to_owned()
        })
    }

    pub fn get_key(name: &str, clusters: &str) -> String {
        if !clusters.is_empty() {
            let key = format!("{}{}{}", name, SERVICE_INFO_SEPARATOR, clusters);
            return key;
        }

        name.to_string()
    }

    /// Get service key without clusters, aligned with Java client's getKeyWithoutClusters().
    /// Used for listener registry subscription/unsubscription to ensure notifications
    /// work correctly regardless of cluster information in server push messages.
    pub fn get_key_without_clusters(service_name: &str, group_name: &str) -> String {
        Self::get_grouped_service_name(service_name, group_name)
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
            all_ips: Some(false),
            all_ips_3x: Some(false),
            reach_protection_threshold: false,
            hosts: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ServiceInfo;
    use crate::api::naming::ServiceInstance;
    use std::sync::Arc;

    #[test]
    fn test_service_info_serde_shared_hosts() {
        let service_info = ServiceInfo {
            hosts: Some(Arc::new(vec![ServiceInstance {
                ip: "127.0.0.1".to_string(),
                port: 8848,
                ..Default::default()
            }])),
            ..Default::default()
        };

        let json = serde_json::to_string(&service_info).expect("service info should serialize");
        assert!(json.contains("\"hosts\":[{"));

        let decoded: ServiceInfo =
            serde_json::from_str(&json).expect("service info should deserialize");
        let hosts = decoded.hosts.expect("hosts should exist");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].port, 8848);
    }

    fn instance(healthy: bool, weight: f64) -> ServiceInstance {
        ServiceInstance {
            ip: "127.0.0.1".to_string(),
            port: 8848,
            healthy,
            weight,
            ..Default::default()
        }
    }

    #[test]
    fn test_validate_boundaries() {
        let mut info = ServiceInfo::default();
        assert!(!info.validate());

        info.all_ips = Some(true);
        assert!(info.validate());

        info.all_ips = Some(false);
        info.all_ips_3x = Some(true);
        assert!(!info.validate());

        info.all_ips = None;
        assert!(info.validate());

        info.all_ips_3x = Some(false);
        info.hosts = Some(Arc::new(vec![instance(false, 1.0)]));
        assert!(!info.validate());

        info.hosts = Some(Arc::new(vec![instance(true, 0.0)]));
        assert!(!info.validate());

        info.hosts = Some(Arc::new(vec![instance(true, 1.0)]));
        assert!(info.validate());
    }

    #[test]
    fn test_ip_count_and_key_format() {
        let mut info = ServiceInfo::default();
        assert_eq!(info.ip_count(), 0);

        info.hosts = Some(Arc::new(vec![instance(true, 1.0), instance(true, 1.0)]));
        assert_eq!(info.ip_count(), 2);

        assert_eq!(
            ServiceInfo::get_grouped_service_name("svc", "DEFAULT"),
            "DEFAULT@@svc"
        );
        assert_eq!(
            ServiceInfo::get_grouped_service_name("DEFAULT@@svc", "DEFAULT"),
            "DEFAULT@@svc"
        );
        assert_eq!(ServiceInfo::get_grouped_service_name("svc", ""), "svc");
        assert_eq!(ServiceInfo::get_key("svc", ""), "svc");
        assert_eq!(ServiceInfo::get_key("svc", "c1"), "svc@@c1");
        assert_eq!(
            ServiceInfo::get_key_without_clusters("svc", "DEFAULT"),
            "DEFAULT@@svc"
        );
    }
}

use std::collections::HashMap;

use nacos_macro::request;
use serde::{Deserialize, Serialize};

#[request(identity = "ConnectionSetupRequest", module = "internal")]
pub(crate) struct ConnectionSetupRequest {
    pub(crate) client_version: String,

    pub(crate) abilities: NacosClientAbilities,

    pub(crate) tenant: String,

    pub(crate) labels: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct NacosClientAbilities {
    #[serde(rename = "remoteAbility")]
    remote_ability: NacosClientRemoteAbility,

    #[serde(rename = "configAbility")]
    config_ability: NacosClientConfigAbility,

    #[serde(rename = "namingAbility")]
    naming_ability: NacosClientNamingAbility,
}

impl NacosClientAbilities {
    pub(crate) fn support_remote_connection(&mut self, enable: bool) {
        self.remote_ability.support_remote_connection(enable);
    }

    pub(crate) fn support_config_remote_metrics(&mut self, enable: bool) {
        self.config_ability.support_remote_metrics(enable);
    }

    pub(crate) fn support_naming_delta_push(&mut self, enable: bool) {
        self.naming_ability.support_delta_push(enable);
    }

    pub(crate) fn support_naming_remote_metric(&mut self, enable: bool) {
        self.naming_ability.support_remote_metric(enable);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct NacosClientRemoteAbility {
    #[serde(rename = "supportRemoteConnection")]
    support_remote_connection: bool,
}

impl NacosClientRemoteAbility {
    fn support_remote_connection(&mut self, enable: bool) {
        self.support_remote_connection = enable;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct NacosClientConfigAbility {
    #[serde(rename = "supportRemoteMetrics")]
    support_remote_metrics: bool,
}

impl NacosClientConfigAbility {
    fn support_remote_metrics(&mut self, enable: bool) {
        self.support_remote_metrics = enable;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct NacosClientNamingAbility {
    #[serde(rename = "supportDeltaPush")]
    support_delta_push: bool,

    #[serde(rename = "supportRemoteMetric")]
    support_remote_metric: bool,
}

impl NacosClientNamingAbility {
    fn support_delta_push(&mut self, enable: bool) {
        self.support_delta_push = enable;
    }

    fn support_remote_metric(&mut self, enable: bool) {
        self.support_remote_metric = enable;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_abilities_are_all_disabled() {
        // The abilities used to be built by a hand-written `new()` that set
        // every flag to false. `Default::default()` must keep producing the
        // same result now that `new()` is gone.
        let json = serde_json::to_value(NacosClientAbilities::default())
            .expect("abilities should serialize");
        assert_eq!(json["remoteAbility"]["supportRemoteConnection"], false);
        assert_eq!(json["configAbility"]["supportRemoteMetrics"], false);
        assert_eq!(json["namingAbility"]["supportDeltaPush"], false);
        assert_eq!(json["namingAbility"]["supportRemoteMetric"], false);
    }

    #[test]
    fn test_abilities_wire_format() {
        // The abilities are sent to the Nacos server in the connection setup
        // request. Pin the exact JSON so a refactor cannot silently change it.
        let json = serde_json::to_value(NacosClientAbilities::default())
            .expect("abilities should serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "remoteAbility": { "supportRemoteConnection": false },
                "configAbility": { "supportRemoteMetrics": false },
                "namingAbility": {
                    "supportDeltaPush": false,
                    "supportRemoteMetric": false,
                },
            })
        );
    }

    #[test]
    fn test_abilities_setters_toggle_flags() {
        let mut abilities = NacosClientAbilities::default();
        abilities.support_remote_connection(true);
        abilities.support_config_remote_metrics(true);
        abilities.support_naming_delta_push(true);
        abilities.support_naming_remote_metric(true);

        let json = serde_json::to_value(&abilities).expect("abilities should serialize");
        assert_eq!(json["remoteAbility"]["supportRemoteConnection"], true);
        assert_eq!(json["configAbility"]["supportRemoteMetrics"], true);
        assert_eq!(json["namingAbility"]["supportDeltaPush"], true);
        assert_eq!(json["namingAbility"]["supportRemoteMetric"], true);
    }
}

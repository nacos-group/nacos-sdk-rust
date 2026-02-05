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
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        NacosClientAbilities {
            remote_ability: NacosClientRemoteAbility::new(),
            config_ability: NacosClientConfigAbility::new(),
            naming_ability: NacosClientNamingAbility::new(),
        }
    }

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
    fn new() -> Self {
        NacosClientRemoteAbility {
            support_remote_connection: false,
        }
    }

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
    fn new() -> Self {
        NacosClientConfigAbility {
            support_remote_metrics: false,
        }
    }

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
    fn new() -> Self {
        NacosClientNamingAbility {
            support_delta_push: false,
            support_remote_metric: false,
        }
    }

    fn support_delta_push(&mut self, enable: bool) {
        self.support_delta_push = enable;
    }

    fn support_remote_metric(&mut self, enable: bool) {
        self.support_remote_metric = enable;
    }
}

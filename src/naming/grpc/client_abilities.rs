use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ClientAbilities {
    #[serde(rename = "remoteAbility")]
    remote_ability: ClientRemoteAbility,

    #[serde(rename = "configAbility")]
    config_ability: ClientConfigAbility,

    #[serde(rename = "namingAbility")]
    naming_ability: ClientNamingAbility,
}

impl ClientAbilities {
    pub(crate) fn new() -> Self {
        ClientAbilities {
            remote_ability: ClientRemoteAbility::new(),
            config_ability: ClientConfigAbility::new(),
            naming_ability: ClientNamingAbility::new(),
        }
    }

    pub(crate) fn support_remote_connection(&mut self, enable: bool) {
        self.remote_ability.support_remote_connection(enable);
    }

    pub(crate) fn support_remote_metrics(&mut self, enable: bool) {
        self.config_ability.support_remote_metrics(enable);
    }

    pub(crate) fn support_delta_push(&mut self, enable: bool) {
        self.naming_ability.support_delta_push(enable);
    }

    pub(crate) fn support_remote_metric(&mut self, enable: bool) {
        self.naming_ability.support_remote_metric(enable);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClientRemoteAbility {
    #[serde(rename = "supportRemoteConnection")]
    support_remote_connection: bool,
}

impl ClientRemoteAbility {
    fn new() -> Self {
        ClientRemoteAbility {
            support_remote_connection: false,
        }
    }

    fn support_remote_connection(&mut self, enable: bool) {
        self.support_remote_connection = enable;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClientConfigAbility {
    #[serde(rename = "supportRemoteMetrics")]
    support_remote_metrics: bool,
}

impl ClientConfigAbility {
    fn new() -> Self {
        ClientConfigAbility {
            support_remote_metrics: false,
        }
    }

    fn support_remote_metrics(&mut self, enable: bool) {
        self.support_remote_metrics = enable;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClientNamingAbility {
    #[serde(rename = "supportDeltaPush")]
    support_delta_push: bool,

    #[serde(rename = "supportRemoteMetric")]
    support_remote_metric: bool,
}

impl ClientNamingAbility {
    fn new() -> Self {
        ClientNamingAbility {
            support_delta_push: false,
            support_remote_metric: false,
        }
    }

    fn support_delta_push(&mut self, enable: bool) {
        self.support_delta_push = enable;
    }

    fn support_remote_metric(&mut self, enable: bool) {
        self.support_delta_push = enable;
    }
}

pub const DEFAULT_SERVER_ADDR: &str = "0.0.0.0:8848";

pub const DEFAULT_SERVER_PORT: u32 = 8848;

/// Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
pub(crate) const DEFAULT_NAMESPACE: &str = "public";

pub const DEFAULT_GROUP: &str = "DEFAULT_GROUP";

pub const UNKNOWN: &str = "unknown";

/// label AppName
pub(crate) const KEY_LABEL_APP_NAME: &str = "AppName";

/// label for crate inner common::remote
pub(crate) mod common_remote {

    pub const LABEL_SOURCE: &str = "source";

    /// LABEL_SOURCE value sdk
    pub const LABEL_SOURCE_SDK: &str = "sdk";

    pub const LABEL_MODULE: &str = "module";

    /// LABEL_MODULE value naming
    pub const LABEL_MODULE_NAMING: &str = "naming";

    /// LABEL_MODULE value config
    pub const LABEL_MODULE_CONFIG: &str = "config";
}

/// env `NACOS_CLIENT_COMMON_THREAD_CORES` to set num when multi-cpus, default is num_cpus
pub const ENV_NACOS_CLIENT_COMMON_THREAD_CORES: &str = "NACOS_CLIENT_COMMON_THREAD_CORES";

pub const ENV_NACOS_CLIENT_COMMON_SERVER_ADDRESS: &str = "nacos.client.server.address";

pub const ENV_NACOS_CLIENT_COMMON_SERVER_PORT: &str = "nacos.client.server.port";

pub const ENV_NACOS_CLIENT_COMMON_NAMESPACE: &str = "nacos.client.namespace";

pub const ENV_NACOS_CLIENT_COMMON_APP_NAME: &str = "nacos.client.app.name";

pub const ENV_NACOS_CLIENT_AUTH_USER_NAME: &str = "nacos.client.username";

pub const ENV_NACOS_CLIENT_AUTH_PASSWORD: &str = "nacos.client.password";

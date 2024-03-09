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

/// env `NACOS_CLIENT_COMMON_THREAD_CORES` to set nacos-client-thread-pool num, default 1
pub const ENV_NACOS_CLIENT_COMMON_THREAD_CORES: &str = "NACOS_CLIENT_COMMON_THREAD_CORES";

pub const ENV_NACOS_CLIENT_COMMON_SERVER_ADDRESS: &str = "NACOS_CLIENT_SERVER_ADDRESS";

pub const ENV_NACOS_CLIENT_COMMON_SERVER_PORT: &str = "NACOS_CLIENT_SERVER_PORT";

pub const ENV_NACOS_CLIENT_COMMON_NAMESPACE: &str = "NACOS_CLIENT_NAMESPACE";

pub const ENV_NACOS_CLIENT_COMMON_APP_NAME: &str = "NACOS_CLIENT_APP_NAME";

pub const ENV_NACOS_CLIENT_AUTH_USERNAME: &str = "NACOS_CLIENT_USERNAME";

pub const ENV_NACOS_CLIENT_AUTH_PASSWORD: &str = "NACOS_CLIENT_PASSWORD";

/// env `NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION`, default true
pub const ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION: &str =
    "NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION";

/// env `NACOS_CLIENT_NAMING_LOAD_CACHE_AT_START`, default false
pub const ENV_NACOS_CLIENT_NAMING_LOAD_CACHE_AT_START: &str =
    "NACOS_CLIENT_NAMING_LOAD_CACHE_AT_START";

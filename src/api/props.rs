use std::{collections::HashMap, path::PathBuf, str::FromStr};

use crate::api::constants::*;
use crate::properties::{get_value, get_value_bool, get_value_option};

/// Configures settings for Client.
#[derive(Debug, Clone)]
pub struct ClientProps {
    /// server_addr e.g: 127.0.0.1:8848; 192.168.0.1
    server_addr: String,
    /// endpoint for resolving server list.
    /// Full URL (http://...) used as-is; bare hostname gets defaults (/nacos/serverlist, port 8080).
    endpoint: Option<String>,
    /// grpc port
    grpc_port: Option<u16>,
    /// public is "", Should define a more meaningful namespace
    namespace: String,
    /// app_name
    app_name: String,
    /// naming push_empty_protection, default true
    naming_push_empty_protection: bool,
    /// naming load_cache_at_start, default false
    naming_load_cache_at_start: bool,
    /// config load_cache_at_start, default false
    config_load_cache_at_start: bool,
    /// Optional root directory for on-disk caches.
    cache_dir: Option<PathBuf>,
    /// Kind of the cache store, default CacheKind::DiskStore
    cache_kind: CacheKind,
    /// env_first when get props, default true
    env_first: bool,
    /// metadata
    labels: HashMap<String, String>,
    /// client_version
    client_version: String,
    /// auth context
    auth_context: HashMap<String, String>,
    /// Maximum retry attempts during initialization phase. Defaults to 1 when None.
    /// Only applies to the connection initialization stage. After successful initialization,
    /// the client will retry indefinitely during runtime to ensure fault tolerance.
    #[deprecated]
    max_retries: Option<u32>,
}

impl ClientProps {
    pub(crate) fn get_server_addr(&self) -> String {
        if self.env_first {
            get_value(
                ENV_NACOS_CLIENT_COMMON_SERVER_ADDRESS,
                self.server_addr.clone(),
            )
        } else {
            self.server_addr.clone()
        }
    }

    pub(crate) fn get_endpoint(&self) -> Option<String> {
        if self.env_first {
            get_value_option(ENV_NACOS_CLIENT_COMMON_ENDPOINT).or_else(|| self.endpoint.clone())
        } else {
            self.endpoint.clone()
        }
    }

    /// The priority of the `endpoint` is higher than `server_addr`
    pub(crate) fn get_address_identifier(&self) -> String {
        self.get_endpoint()
            .unwrap_or_else(|| self.get_server_addr())
    }

    pub(crate) fn get_remote_grpc_port(&self) -> Option<u16> {
        self.grpc_port
    }

    pub(crate) fn get_namespace_default_if_empty(&self) -> String {
        let namespace = self.get_namespace();
        if namespace.is_empty() {
            DEFAULT_NAMESPACE.to_owned()
        } else {
            namespace
        }
    }

    pub(crate) fn get_namespace(&self) -> String {
        if self.env_first {
            get_value(ENV_NACOS_CLIENT_COMMON_NAMESPACE, self.namespace.clone())
        } else {
            self.namespace.clone()
        }
    }

    pub(crate) fn get_app_name(&self) -> String {
        if self.env_first {
            get_value(ENV_NACOS_CLIENT_COMMON_APP_NAME, self.app_name.clone())
        } else {
            self.app_name.clone()
        }
    }

    pub(crate) fn get_naming_push_empty_protection(&self) -> bool {
        if self.env_first {
            get_value_bool(
                ENV_NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION,
                self.naming_push_empty_protection,
            )
        } else {
            self.naming_push_empty_protection
        }
    }

    pub(crate) fn get_naming_load_cache_at_start(&self) -> bool {
        if self.env_first {
            get_value_bool(
                ENV_NACOS_CLIENT_NAMING_LOAD_CACHE_AT_START,
                self.naming_load_cache_at_start,
            )
        } else {
            self.naming_load_cache_at_start
        }
    }

    pub(crate) fn get_config_load_cache_at_start(&self) -> bool {
        if self.env_first {
            get_value_bool(
                ENV_NACOS_CLIENT_CONFIG_LOAD_CACHE_AT_START,
                self.config_load_cache_at_start,
            )
        } else {
            self.config_load_cache_at_start
        }
    }

    pub(crate) fn get_cache_dir(&self) -> Option<PathBuf> {
        if self.env_first {
            get_value_option(ENV_NACOS_CLIENT_CACHE_DIR)
                .filter(|cache_dir| !cache_dir.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| self.cache_dir.clone())
        } else {
            self.cache_dir.clone()
        }
    }

    pub(crate) fn get_cache_kind(&self) -> CacheKind {
        if self.env_first
            && let Some(value) = get_value_option(ENV_NACOS_CLIENT_CACHE_KIND)
            && !value.trim().is_empty()
        {
            return match CacheKind::from_str(&value) {
                Ok(cache_kind) => cache_kind,
                Err(_) => self.cache_kind,
            };
        }
        self.cache_kind
    }

    pub(crate) fn get_labels(&self) -> HashMap<String, String> {
        let mut labels = self.labels.clone();
        labels.insert(KEY_LABEL_APP_NAME.to_string(), self.get_app_name());
        labels
    }

    pub(crate) fn get_client_version(&self) -> String {
        self.client_version.clone()
    }

    pub(crate) fn get_auth_context(&self) -> HashMap<String, String> {
        let mut auth_context = self.auth_context.clone();
        if self.env_first {
            #[cfg(feature = "auth-by-http")]
            self.get_http_auth_context(&mut auth_context);
            #[cfg(feature = "auth-by-aliyun")]
            self.get_aliyun_auth_context(&mut auth_context);
        }
        auth_context
    }

    #[cfg(feature = "auth-by-http")]
    fn get_http_auth_context(&self, context: &mut HashMap<String, String>) {
        if let Some(u) = get_value_option(ENV_NACOS_CLIENT_AUTH_USERNAME) {
            context.insert(crate::api::plugin::USERNAME.into(), u);
        }
        if let Some(p) = get_value_option(ENV_NACOS_CLIENT_AUTH_PASSWORD) {
            context.insert(crate::api::plugin::PASSWORD.into(), p);
        }
    }

    #[cfg(feature = "auth-by-aliyun")]
    fn get_aliyun_auth_context(&self, context: &mut HashMap<String, String>) {
        if let Some(ak) = get_value_option(ENV_NACOS_CLIENT_AUTH_ACCESS_KEY) {
            context.insert(crate::api::plugin::ACCESS_KEY.into(), ak);
        }
        if let Some(sk) = get_value_option(ENV_NACOS_CLIENT_AUTH_ACCESS_SECRET) {
            context.insert(crate::api::plugin::ACCESS_SECRET.into(), sk);
        }
        if let Some(sign_region_id) = get_value_option(ENV_NACOS_CLIENT_SIGN_REGION_ID) {
            context.insert(crate::api::plugin::SIGN_REGION_ID.into(), sign_region_id);
        }
    }

    pub(crate) fn get_max_retries(&self) -> Option<u32> {
        #[allow(deprecated)]
        self.max_retries
    }
}

#[allow(clippy::new_without_default)]
impl ClientProps {
    /// Creates a new `ClientConfig`.
    pub fn new() -> Self {
        let env_project_version = env!("CARGO_PKG_VERSION");
        let client_version = format!("Nacos-Rust-Client:{}", env_project_version);

        ClientProps {
            server_addr: String::from(DEFAULT_SERVER_ADDR),
            endpoint: None,
            namespace: String::from(""),
            app_name: UNKNOWN.to_string(),
            naming_push_empty_protection: true,
            naming_load_cache_at_start: false,
            config_load_cache_at_start: false,
            cache_dir: None,
            cache_kind: CacheKind::default(),
            env_first: true,
            labels: HashMap::default(),
            client_version,
            auth_context: HashMap::default(),
            grpc_port: None,
            #[allow(deprecated)]
            max_retries: None,
        }
    }

    /// Sets the server addr.
    pub fn server_addr(mut self, server_addr: impl Into<String>) -> Self {
        self.server_addr = server_addr.into();
        self
    }

    /// Sets the endpoint used to resolve server addresses.
    ///
    /// Full URL (e.g. `http://addr:8080/nacos/serverlist`) is used as-is,
    /// only appending `namespace` if missing from the query string.
    /// Bare hostname (e.g. `addr` or `addr:9090`) gets default path
    /// `/nacos/serverlist` and port 8080.
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Sets the grpc port
    pub fn remote_grpc_port(mut self, grpc_port: u16) -> Self {
        self.grpc_port = Some(grpc_port);
        self
    }

    /// Sets the namespace.
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Sets the app_name.
    pub fn app_name(mut self, app_name: impl Into<String>) -> Self {
        self.app_name = app_name.into();
        self
    }

    /// Sets the naming_push_empty_protection.
    pub fn naming_push_empty_protection(mut self, naming_push_empty_protection: bool) -> Self {
        self.naming_push_empty_protection = naming_push_empty_protection;
        self
    }

    /// Sets the naming_load_cache_at_start.
    pub fn naming_load_cache_at_start(mut self, naming_load_cache_at_start: bool) -> Self {
        self.naming_load_cache_at_start = naming_load_cache_at_start;
        self
    }

    /// Sets the config_load_cache_at_start.
    pub fn config_load_cache_at_start(mut self, config_load_cache_at_start: bool) -> Self {
        self.config_load_cache_at_start = config_load_cache_at_start;
        self
    }

    /// Sets the config_load_cache_at_start / naming_load_cache_at_start.
    pub fn load_cache_at_start(mut self, load_cache_at_start: bool) -> Self {
        self.naming_load_cache_at_start = load_cache_at_start;
        self.config_load_cache_at_start = load_cache_at_start;
        self
    }

    /// Sets the root directory used for on-disk caches.
    ///
    /// The module and namespace directories are appended to this path.
    /// When `env_first` is enabled, `NACOS_CLIENT_CACHE_DIR` takes precedence.
    pub fn cache_dir(mut self, cache_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    /// Sets the kind of the cache store.
    ///
    /// - [`CacheKind::DiskStore`]: persists the cache into disk files(default),
    ///   the root directory could be set by [`ClientProps::cache_dir`].
    /// - [`CacheKind::None`]: keeps the cache in memory only, nothing is written
    ///   to disk. Note `load_cache_at_start(true)` gets nothing to load with this kind.
    ///
    /// When `env_first` is enabled, `NACOS_CLIENT_CACHE_KIND` takes precedence,
    /// e.g. `NACOS_CLIENT_CACHE_KIND=none` disables the disk store.
    pub fn cache_kind(mut self, cache_kind: CacheKind) -> Self {
        self.cache_kind = cache_kind;
        self
    }

    /// Sets the env_first.
    pub fn env_first(mut self, env_first: bool) -> Self {
        self.env_first = env_first;
        self
    }

    /// Sets the labels.
    pub fn labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Add auth username.
    #[cfg(feature = "auth-by-http")]
    pub fn auth_username(mut self, username: impl Into<String>) -> Self {
        self.auth_context
            .insert(crate::api::plugin::USERNAME.into(), username.into());
        self
    }

    /// Add auth password.
    #[cfg(feature = "auth-by-http")]
    pub fn auth_password(mut self, password: impl Into<String>) -> Self {
        self.auth_context
            .insert(crate::api::plugin::PASSWORD.into(), password.into());
        self
    }

    /// Add access-key
    #[cfg(feature = "auth-by-aliyun")]
    pub fn auth_access_key(mut self, access_key: impl Into<String>) -> Self {
        self.auth_context
            .insert(crate::api::plugin::ACCESS_KEY.into(), access_key.into());
        self
    }

    /// Add access-secret
    #[cfg(feature = "auth-by-aliyun")]
    pub fn auth_access_secret(mut self, access_secret: impl Into<String>) -> Self {
        self.auth_context.insert(
            crate::api::plugin::ACCESS_SECRET.into(),
            access_secret.into(),
        );
        self
    }

    /// Add signature region id
    #[cfg(feature = "auth-by-aliyun")]
    pub fn auth_signature_region_id(mut self, signature_region_id: impl Into<String>) -> Self {
        self.auth_context.insert(
            crate::api::plugin::SIGN_REGION_ID.into(),
            signature_region_id.into(),
        );
        self
    }

    /// Add auth ext params.
    pub fn auth_ext(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.auth_context.insert(key.into(), val.into());
        self
    }

    /// Sets the maximum retry attempts during initialization phase.
    ///
    /// This value only applies to the connection initialization stage.
    /// If not set, defaults to 1 retry attempts.
    /// After successful initialization, the client will retry indefinitely
    /// during runtime to ensure fault tolerance.
    #[deprecated]
    #[allow(deprecated)]
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }
}

/// Kind of the store backing the SDK local cache(config & naming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheKind {
    /// Persists the cache into disk files, default.
    #[default]
    DiskStore,
    /// Keeps the cache in memory only, nothing is written to disk.
    ///
    /// Attention! `load_cache_at_start(true)` would have nothing to load,
    /// so the emergency startup mode is not available with this kind.
    None,
}

impl CacheKind {
    /// Display name of this cache kind.
    pub fn name(&self) -> &'static str {
        match self {
            CacheKind::DiskStore => "disk-store",
            CacheKind::None => "none",
        }
    }

    /// Checks whether the cache is persisted into disk.
    pub fn is_disk_store(&self) -> bool {
        matches!(self, CacheKind::DiskStore)
    }
}

impl FromStr for CacheKind {
    type Err = crate::api::error::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "diskstore" | "disk-store" | "disk_store" | "disk" => Ok(CacheKind::DiskStore),
            "none" | "disabled" | "memory" | "memory-only" => Ok(CacheKind::None),
            _ => Err(crate::api::error::Error::InvalidParam(
                "cache_kind".into(),
                format!(
                    "unknown cache kind: {value}, expect one of \
                     [disk-store, diskstore, disk_store, disk, none, disabled, memory, memory-only]"
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::error::Error;

    use super::*;

    #[tokio::test]
    async fn test_get_server_list() {
        let client_props = ClientProps::new()
            .server_addr("127.0.0.1:8848,192.168.0.1")
            .namespace("test_namespace");

        let provider =
            crate::common::remote::server_list::create_server_list_provider(&client_props)
                .await
                .expect("provider should be created");
        let result = provider.current_server_list().await;
        assert!(result.contains(&"127.0.0.1:8848".to_string()));
        assert!(result.contains(&"192.168.0.1:8848".to_string()));

        let client_props = ClientProps::new().server_addr("     ");
        let result1 =
            crate::common::remote::server_list::create_server_list_provider(&client_props).await;
        assert!(result1.is_err());
        let err = match result1 {
            Ok(_) => panic!("expected error result"),
            Err(err) => err,
        };
        assert_eq!(
            format!("{}", err),
            format!(
                "{}",
                Error::WrongServerAddress("Server address is empty".to_string())
            )
        );
    }

    #[test]
    fn test_get_endpoint() {
        let props = ClientProps::new().endpoint("http://127.0.0.1:8080");
        assert_eq!(
            props.get_endpoint().as_deref(),
            Some("http://127.0.0.1:8080")
        );
    }

    #[test]
    fn test_address_identifier_prefers_endpoint() {
        let props = ClientProps::new()
            .server_addr("10.0.0.1:8848,10.0.0.2:8848,10.0.0.3:8848")
            .endpoint("http://endpoint.example.com:8080/nacos/serverlist");
        assert_eq!(
            props.get_address_identifier(),
            "http://endpoint.example.com:8080/nacos/serverlist"
        );

        let props = ClientProps::new().server_addr("10.0.0.1:8848");
        assert_eq!(props.get_address_identifier(), "10.0.0.1:8848");
    }

    #[test]
    fn test_get_cache_dir_from_env() {
        const CHILD_ENV: &str = "NACOS_CLIENT_CACHE_DIR_TEST_CHILD";

        if std::env::var_os(CHILD_ENV).is_none() {
            let status = std::process::Command::new(
                std::env::current_exe().expect("current test executable should be available"),
            )
            .args(["--exact", "api::props::tests::test_get_cache_dir_from_env"])
            .env(CHILD_ENV, "1")
            .env(ENV_NACOS_CLIENT_CACHE_DIR, "env-cache")
            .status()
            .expect("cache directory environment test should run");
            assert!(status.success());
            return;
        }

        let props = ClientProps::new().cache_dir("builder-cache");
        assert_eq!(props.get_cache_dir(), Some(PathBuf::from("env-cache")));

        let props = ClientProps::new()
            .cache_dir("builder-cache")
            .env_first(false);
        assert_eq!(props.get_cache_dir(), Some(PathBuf::from("builder-cache")));
    }

    #[test]
    fn test_cache_kind_from_str() {
        for value in [
            "DiskStore",
            "diskstore",
            "disk-store",
            "DISK_STORE",
            " disk ",
        ] {
            assert_eq!(
                CacheKind::from_str(value).expect("value should be parsed"),
                CacheKind::DiskStore,
                "value: {value}"
            );
        }

        for value in [
            "none",
            "None",
            "NONE",
            "disabled",
            " memory ",
            "memory-only",
        ] {
            assert_eq!(
                CacheKind::from_str(value).expect("value should be parsed"),
                CacheKind::None,
                "value: {value}"
            );
        }

        for value in ["", "  ", "redis", "memoryonly", "true"] {
            let err = match CacheKind::from_str(value) {
                Ok(_) => panic!("value {value} should not be parsed"),
                Err(err) => err,
            };
            assert!(matches!(err, Error::InvalidParam(_, _)), "value: {value}");
        }
    }

    #[test]
    fn test_cache_kind_default_and_setter() {
        assert_eq!(ClientProps::new().get_cache_kind(), CacheKind::DiskStore);
        assert_eq!(CacheKind::default(), CacheKind::DiskStore);
        assert!(CacheKind::DiskStore.is_disk_store());
        assert!(!CacheKind::None.is_disk_store());
        assert_eq!(CacheKind::DiskStore.name(), "disk-store");
        assert_eq!(CacheKind::None.name(), "none");

        let props = ClientProps::new()
            .cache_kind(CacheKind::None)
            .env_first(false);
        assert_eq!(props.get_cache_kind(), CacheKind::None);
    }

    /// `PROPERTIES` is a process-wide snapshot, so env driven cases run in child processes,
    /// one child per `NACOS_CLIENT_CACHE_KIND` value.
    #[test]
    fn test_get_cache_kind_from_env() {
        const CHILD_CASE_ENV: &str = "NACOS_CLIENT_CACHE_KIND_TEST_CASE";

        let Ok(case) = std::env::var(CHILD_CASE_ENV) else {
            let current_exe =
                std::env::current_exe().expect("current test executable should be available");
            for (case, env_value) in [
                ("none", "none"),
                ("disk-store", "disk-store"),
                ("invalid", "redis"),
                ("blank", "   "),
            ] {
                let status = std::process::Command::new(&current_exe)
                    .args(["--exact", "api::props::tests::test_get_cache_kind_from_env"])
                    .env(CHILD_CASE_ENV, case)
                    .env(ENV_NACOS_CLIENT_CACHE_KIND, env_value)
                    .status()
                    .expect("cache kind environment test should run");
                assert!(status.success(), "child case {case} should pass");
            }
            return;
        };

        match case.as_str() {
            // env takes priority over the props value
            "none" => {
                let props = ClientProps::new().cache_kind(CacheKind::DiskStore);
                assert_eq!(props.get_cache_kind(), CacheKind::None);

                // env_first disabled keeps the props value
                let props = ClientProps::new()
                    .cache_kind(CacheKind::DiskStore)
                    .env_first(false);
                assert_eq!(props.get_cache_kind(), CacheKind::DiskStore);
            }
            "disk-store" => {
                let props = ClientProps::new().cache_kind(CacheKind::None);
                assert_eq!(props.get_cache_kind(), CacheKind::DiskStore);

                let props = ClientProps::new()
                    .cache_kind(CacheKind::None)
                    .env_first(false);
                assert_eq!(props.get_cache_kind(), CacheKind::None);
            }
            // unknown value falls back to the props value, then to the default
            "invalid" => {
                let props = ClientProps::new().cache_kind(CacheKind::None);
                assert_eq!(props.get_cache_kind(), CacheKind::None);

                let props = ClientProps::new().cache_kind(CacheKind::DiskStore);
                assert_eq!(props.get_cache_kind(), CacheKind::DiskStore);
            }
            // blank value falls back to the props value, then to the default
            "blank" => {
                let props = ClientProps::new().cache_kind(CacheKind::None);
                assert_eq!(props.get_cache_kind(), CacheKind::None);

                let props = ClientProps::new();
                assert_eq!(props.get_cache_kind(), CacheKind::DiskStore);
            }
            other => panic!("unknown test case: {other}"),
        }
    }
}

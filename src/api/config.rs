use crate::api::{error, plugin, props};
use std::collections::HashMap;
use std::sync::Arc;

/// Api [`ConfigService`].
///
/// # Examples
///
/// ```no_run
/// # async fn run() -> nacos_sdk::api::error::Result<()> {
/// let config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
///       nacos_sdk::api::props::ClientProps::new()
///          .server_addr("127.0.0.1:8848")
///          // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///          .namespace("")
///          .app_name("todo-your-app-name"),
///  )
///  .build()
///  .await?;
/// # Ok(())
/// # }
/// ```
#[doc(alias("config", "sdk", "api"))]
#[derive(Clone, Debug)]
pub struct ConfigService {
    inner: Arc<crate::config::NacosConfigService>,
}

impl ConfigService {
    /// Get config, return the content.
    ///
    /// Attention to [`error::Error::ConfigNotFound`], [`error::Error::ConfigQueryConflict`]
    pub async fn get_config(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
    ) -> error::Result<ConfigResponse> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.get_config(data_id, group).await
    }

    /// Publish config, return true/false.
    pub async fn publish_config(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
        content: impl Into<String>,
        content_type: Option<String>,
    ) -> error::Result<bool> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        let content: String = content.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        crate::common::util::check_not_blank(&content, "content")?;
        self.inner
            .publish_config(data_id, group, content, content_type)
            .await
    }

    /// Cas publish config with cas_md5 (prev content's md5), return true/false.
    pub async fn publish_config_cas(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
        content: impl Into<String>,
        content_type: Option<String>,
        cas_md5: impl Into<String>,
    ) -> error::Result<bool> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        let content: String = content.into();
        let cas_md5: String = cas_md5.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        crate::common::util::check_not_blank(&content, "content")?;
        crate::common::util::check_not_blank(&cas_md5, "cas_md5")?;
        self.inner
            .publish_config_cas(data_id, group, content, content_type, cas_md5)
            .await
    }

    /// Beta publish config, return true/false.
    pub async fn publish_config_beta(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
        content: impl Into<String>,
        content_type: Option<String>,
        beta_ips: impl Into<String>,
    ) -> error::Result<bool> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        let content: String = content.into();
        let beta_ips: String = beta_ips.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        crate::common::util::check_not_blank(&content, "content")?;
        crate::common::util::check_not_blank(&beta_ips, "beta_ips")?;
        self.inner
            .publish_config_beta(data_id, group, content, content_type, beta_ips)
            .await
    }

    /// Publish config with params (see keys [`constants::*`]), return true/false.
    pub async fn publish_config_param(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
        content: impl Into<String>,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: HashMap<String, String>,
    ) -> error::Result<bool> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        let content: String = content.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        crate::common::util::check_not_blank(&content, "content")?;
        self.inner
            .publish_config_param(data_id, group, content, content_type, cas_md5, params)
            .await
    }

    /// Remove config, return true/false.
    pub async fn remove_config(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
    ) -> error::Result<bool> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.remove_config(data_id, group).await
    }

    /// Listen the config change.
    pub async fn add_listener(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
        listener: Arc<dyn ConfigChangeListener>,
    ) -> error::Result<()> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.add_listener(data_id, group, listener).await
    }

    /// Remove a Listener.
    pub async fn remove_listener(
        &self,
        data_id: impl Into<String>,
        group: impl Into<String>,
        listener: Arc<dyn ConfigChangeListener>,
    ) -> error::Result<()> {
        let data_id: String = data_id.into();
        let group: String = group.into();
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.remove_listener(data_id, group, listener).await
    }
}

/// The ConfigChangeListener receive notify of [`ConfigResponse`].
pub trait ConfigChangeListener: Send + Sync {
    fn notify(&self, config_resp: ConfigResponse);
}

/// ConfigResponse for api.
#[derive(Debug, Clone)]
pub struct ConfigResponse {
    /// Namespace/Tenant
    namespace: Arc<String>,
    /// DataId
    data_id: Arc<String>,
    /// Group
    group: Arc<String>,
    /// Content
    content: Arc<String>,
    /// Content's Type; e.g. json,properties,xml,html,text,yaml
    content_type: Arc<String>,
    /// Content's md5
    md5: Arc<String>,
}

impl std::fmt::Display for ConfigResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConfigResponse(namespace={n},data_id={d},group={g},md5={m},content=",
            n = self.namespace,
            d = self.data_id,
            g = self.group,
            m = self.md5,
        )?;
        // Truncate content for display if it exceeds 30 chars
        if self.content.chars().count() > 30 {
            for c in self.content.chars().take(30) {
                write!(f, "{}", c)?;
            }
            write!(f, "...")?;
        } else {
            write!(f, "{}", self.content)?;
        }
        write!(f, ")")
    }
}

impl ConfigResponse {
    pub fn new(
        data_id: impl Into<String>,
        group: impl Into<String>,
        namespace: impl Into<String>,
        content: impl Into<String>,
        content_type: impl Into<String>,
        md5: impl Into<String>,
    ) -> Self {
        ConfigResponse {
            data_id: Arc::new(data_id.into()),
            group: Arc::new(group.into()),
            namespace: Arc::new(namespace.into()),
            content: Arc::new(content.into()),
            content_type: Arc::new(content_type.into()),
            md5: Arc::new(md5.into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn shares_content_with(&self, other: &Arc<String>) -> bool {
        Arc::ptr_eq(&self.content, other)
    }

    #[cfg(test)]
    pub(crate) fn shares_content_type_with(&self, other: &Arc<String>) -> bool {
        Arc::ptr_eq(&self.content_type, other)
    }

    #[cfg(test)]
    pub(crate) fn shares_md5_with(&self, other: &Arc<String>) -> bool {
        Arc::ptr_eq(&self.md5, other)
    }

    pub(crate) fn from_shared(
        data_id: Arc<String>,
        group: Arc<String>,
        namespace: Arc<String>,
        content: Arc<String>,
        content_type: Arc<String>,
        md5: Arc<String>,
    ) -> Self {
        ConfigResponse {
            data_id,
            group,
            namespace,
            content,
            content_type,
            md5,
        }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn data_id(&self) -> &str {
        &self.data_id
    }
    pub fn group(&self) -> &str {
        &self.group
    }
    pub fn content(&self) -> &str {
        &self.content
    }
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
    pub fn md5(&self) -> &str {
        &self.md5
    }
}

pub mod constants {
    /// param type, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_CONTENT_TYPE: &str = "type";

    /// param betaIps, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_BETA_IPS: &str = "betaIps";

    /// param appName, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_APP_NAME: &str = "appName";

    /// param tag, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_TAG: &str = "tag";

    /// param encryptedDataKey, use inner.
    pub(crate) const KEY_PARAM_ENCRYPTED_DATA_KEY: &str = "encryptedDataKey";
}

/// Builder of api [`ConfigService`].
///
/// # Examples
///
/// ```no_run
/// # async fn run() -> nacos_sdk::api::error::Result<()> {
/// let config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
///       nacos_sdk::api::props::ClientProps::new()
///          .server_addr("127.0.0.1:8848")
///          // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///          .namespace("")
///          .app_name("todo-your-app-name"),
///  )
///  .build()
///  .await?;
/// # Ok(())
/// # }
/// ```
#[doc(alias("config", "builder"))]
pub struct ConfigServiceBuilder {
    client_props: props::ClientProps,
    auth_plugin: Option<Arc<dyn plugin::AuthPlugin>>,
    config_filters: Vec<Box<dyn plugin::ConfigFilter>>,
}

impl Default for ConfigServiceBuilder {
    fn default() -> Self {
        ConfigServiceBuilder {
            client_props: props::ClientProps::new(),
            auth_plugin: None,
            config_filters: Vec::new(),
        }
    }
}

impl ConfigServiceBuilder {
    pub fn new(client_props: props::ClientProps) -> Self {
        ConfigServiceBuilder {
            client_props,
            auth_plugin: None,
            config_filters: Vec::new(),
        }
    }

    #[cfg(feature = "auth-by-http")]
    pub fn enable_auth_plugin_http(self) -> Self {
        self.with_auth_plugin(Arc::new(plugin::HttpLoginAuthPlugin::default()))
    }

    #[cfg(feature = "auth-by-aliyun")]
    pub fn enable_auth_plugin_aliyun(self) -> Self {
        self.with_auth_plugin(Arc::new(plugin::AliyunRamAuthPlugin::default()))
    }

    /// Set [`plugin::AuthPlugin`]
    pub fn with_auth_plugin(mut self, auth_plugin: Arc<dyn plugin::AuthPlugin>) -> Self {
        self.auth_plugin = Some(auth_plugin);
        self
    }

    pub fn with_config_filters(
        mut self,
        config_filters: Vec<Box<dyn plugin::ConfigFilter>>,
    ) -> Self {
        self.config_filters = config_filters;
        self
    }

    pub fn add_config_filter(mut self, config_filter: Box<dyn plugin::ConfigFilter>) -> Self {
        self.config_filters.push(config_filter);
        self
    }

    /// Add [`plugin::EncryptionPlugin`], they will wrapper with [`plugin::ConfigEncryptionFilter`] into [`config_filters`]
    pub fn with_encryption_plugins(
        self,
        encryption_plugins: Vec<Box<dyn plugin::EncryptionPlugin>>,
    ) -> Self {
        self.add_config_filter(Box::new(plugin::ConfigEncryptionFilter::new(
            encryption_plugins,
        )))
    }

    /// Builds a new [`ConfigService`].
    pub async fn build(self) -> error::Result<ConfigService> {
        #[cfg(feature = "tracing-log")]
        {
            // $HOME/logs/nacos
            let log_path = crate::common::util::HOME_DIR.to_owned() + "/logs/nacos";
            let log_level = "INFO".to_string();
            crate::common::log::init(log_path, log_level);
        }

        let auth_plugin = match self.auth_plugin {
            None => Arc::new(plugin::NoopAuthPlugin::default()),
            Some(plugin) => plugin,
        };
        let inner = crate::config::NacosConfigService::new(
            self.client_props,
            auth_plugin,
            self.config_filters,
        )
        .await?;
        let inner = Arc::new(inner);
        Ok(ConfigService { inner })
    }
}

#[cfg(test)]
mod tests {
    use crate::api::config::ConfigServiceBuilder;
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_config_response_accessors_accept_str() {
        let resp = ConfigResponse::new("data-id", "group", "namespace", "content", "text", "md5");
        let data_id: &str = resp.data_id();
        let group: &str = resp.group();
        let namespace: &str = resp.namespace();
        let content: &str = resp.content();
        let content_type: &str = resp.content_type();
        let md5: &str = resp.md5();
        assert_eq!(data_id, "data-id");
        assert_eq!(group, "group");
        assert_eq!(namespace, "namespace");
        assert_eq!(content, "content");
        assert_eq!(content_type, "text");
        assert_eq!(md5, "md5");
    }

    #[test]
    fn test_config_response_display_truncation() {
        let content_30 = "a".repeat(30);
        let resp = ConfigResponse::new("d", "g", "n", content_30.clone(), "text", "m");
        assert_eq!(
            format!("{resp}"),
            format!("ConfigResponse(namespace=n,data_id=d,group=g,md5=m,content={content_30})")
        );

        let resp = ConfigResponse::new("d", "g", "n", "a".repeat(31), "text", "m");
        assert!(format!("{resp}").ends_with(&format!("content={}...)", "a".repeat(30))));

        let resp = ConfigResponse::new("d", "g", "n", "配".repeat(31), "text", "m");
        assert!(format!("{resp}").ends_with(&format!("content={}...)", "配".repeat(30))));
    }

    struct TestConfigChangeListener;

    impl ConfigChangeListener for TestConfigChangeListener {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::info!("listen the config={}", config_resp);
        }
    }

    fn tracing_log_try_init() {
        crate::test_config::setup_log();
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service() {
        tracing_log_try_init();

        let (data_id, group) = ("test_api_config_service".to_string(), "TEST".to_string());

        let config_service = ConfigServiceBuilder::default()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        // publish a config
        let _publish_resp = config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                "test_api_config_service".to_string(),
                Some("text".to_string()),
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        // sleep for config sync in server
        sleep(Duration::from_millis(111)).await;

        let config = config_service
            .get_config(data_id.clone(), group.clone())
            .await;
        match config {
            Ok(config) => tracing::info!("get the config {}", config),
            Err(err) => tracing::error!("get the config {:?}", err),
        }

        let _listen = config_service
            .add_listener(
                data_id.clone(),
                group.clone(),
                std::sync::Arc::new(TestConfigChangeListener {}),
            )
            .await;
        match _listen {
            Ok(_) => tracing::info!("listening the config success"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        // publish a config for listener
        let _publish_resp = config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                "test_api_config_service_for_listener".to_string(),
                Some("text".to_string()),
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        // example get a config not exit
        let config_resp = config_service
            .get_config("todo-data-id".to_string(), "todo-group".to_string())
            .await;
        match config_resp {
            Ok(config_resp) => tracing::info!("get the config {}", config_resp),
            Err(err) => tracing::error!("get the config {:?}", err),
        }

        // example add a listener with config not exit
        let _listen = config_service
            .add_listener(
                "todo-data-id".to_string(),
                "todo-group".to_string(),
                std::sync::Arc::new(TestConfigChangeListener {}),
            )
            .await;
        match _listen {
            Ok(_) => tracing::info!("listening the config success"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        // sleep for listener
        sleep(Duration::from_millis(111)).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_remove_config() {
        tracing_log_try_init();

        let config_service = ConfigServiceBuilder::default()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        // remove a config not exit
        let remove_resp = config_service
            .remove_config("todo-data-id".to_string(), "todo-group".to_string())
            .await;
        match remove_resp {
            Ok(result) => tracing::info!("remove a config not exit: {}", result),
            Err(err) => tracing::error!("remove a config not exit: {:?}", err),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_publish_config() {
        tracing_log_try_init();

        let config_service = ConfigServiceBuilder::default()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        // publish a config
        let publish_resp = config_service
            .publish_config(
                "test_api_config_service_publish_config".to_string(),
                "TEST".to_string(),
                "test_api_config_service_publish_config".to_string(),
                Some("text".to_string()),
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        tracing::info!("publish a config: {}", publish_resp);
        assert!(publish_resp);
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_publish_config_param() {
        tracing_log_try_init();

        let config_service = ConfigServiceBuilder::default()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        let mut params = HashMap::new();
        params.insert(
            crate::api::config::constants::KEY_PARAM_APP_NAME.into(),
            "test".into(),
        );
        // publish a config with param
        let publish_resp = config_service
            .publish_config_param(
                "test_api_config_service_publish_config_param".to_string(),
                "TEST".to_string(),
                "test_api_config_service_publish_config_param".to_string(),
                None,
                None,
                params,
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        tracing::info!("publish a config with param: {}", publish_resp);
        assert!(publish_resp);
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_publish_config_beta() {
        tracing_log_try_init();

        let config_service = ConfigServiceBuilder::default()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        // publish a config with beta
        let publish_resp = config_service
            .publish_config_beta(
                "test_api_config_service_publish_config".to_string(),
                "TEST".to_string(),
                "test_api_config_service_publish_config_beta".to_string(),
                None,
                "127.0.0.1,192.168.0.1".to_string(),
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        tracing::info!("publish a config with beta: {}", publish_resp);
        assert!(publish_resp);
    }

    #[tokio::test]
    #[ignore]
    #[cfg(not(tarpaulin))]
    async fn test_api_config_service_publish_config_cas() {
        tracing_log_try_init();

        let config_service = ConfigServiceBuilder::default()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        let data_id = "test_api_config_service_publish_config_cas".to_string();
        let group = "TEST".to_string();
        // publish a config
        let publish_resp = config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                "test_api_config_service_publish_config_cas".to_string(),
                None,
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        assert!(publish_resp);

        // sleep for config sync in server
        sleep(Duration::from_millis(111)).await;

        // get a config
        let config_resp = config_service
            .get_config(data_id.clone(), group.clone())
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");

        // publish a config with cas
        let content_cas_md5 =
            "test_api_config_service_publish_config_cas_md5_".to_string() + config_resp.md5();
        let publish_resp = config_service
            .publish_config_cas(
                data_id.clone(),
                group.clone(),
                content_cas_md5.clone(),
                None,
                config_resp.md5().to_string(),
            )
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        tracing::info!("publish a config with cas: {}", publish_resp);
        assert!(publish_resp);

        // publish a config with cas md5 not right
        let content_cas_md5_not_right = "test_api_config_service_publish_config_cas_md5_not_right";
        let publish_resp = config_service
            .publish_config_cas(
                data_id.clone(),
                group.clone(),
                content_cas_md5_not_right.to_string(),
                None,
                config_resp.md5().to_string(),
            )
            .await;
        match publish_resp {
            Ok(result) => tracing::info!("publish a config with cas: {}", result),
            Err(err) => tracing::error!("publish a config with cas: {:?}", err),
        }
        sleep(Duration::from_millis(111)).await;

        let config_resp = config_service
            .get_config(data_id.clone(), group.clone())
            .await
            .expect("ConfigServiceBuilder should build successfully in tests");
        assert_ne!(content_cas_md5_not_right, config_resp.content());
        assert_eq!(content_cas_md5.as_str(), config_resp.content());
    }
}

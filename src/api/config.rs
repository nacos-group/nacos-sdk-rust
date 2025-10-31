use crate::api::{error, plugin, props};
use std::collections::HashMap;
use std::sync::Arc;

/// Api [`ConfigService`].
///
/// # Examples
///
/// ```ignore
///  let mut config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
///        nacos_sdk::api::props::ClientProps::new()
///           .server_addr("127.0.0.1:8848")
///           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///           .namespace("")
///           .app_name("todo-your-app-name"),
///   )
///   .build()?;
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
        data_id: String,
        group: String,
    ) -> error::Result<ConfigResponse> {
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.get_config(data_id, group).await
    }

    /// Publish config, return true/false.
    pub async fn publish_config(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> error::Result<bool> {
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
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> error::Result<bool> {
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
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> error::Result<bool> {
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
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: HashMap<String, String>,
    ) -> error::Result<bool> {
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        crate::common::util::check_not_blank(&content, "content")?;
        self.inner
            .publish_config_param(data_id, group, content, content_type, cas_md5, params)
            .await
    }

    /// Remove config, return true/false.
    pub async fn remove_config(&self, data_id: String, group: String) -> error::Result<bool> {
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.remove_config(data_id, group).await
    }

    /// Listen the config change.
    pub async fn add_listener(
        &self,
        data_id: String,
        group: String,
        listener: Arc<dyn ConfigChangeListener>,
    ) -> error::Result<()> {
        crate::common::util::check_not_blank(&data_id, "data_id")?;
        crate::common::util::check_not_blank(&group, "group")?;
        self.inner.add_listener(data_id, group, listener).await
    }

    /// Remove a Listener.
    pub async fn remove_listener(
        &self,
        data_id: String,
        group: String,
        listener: Arc<dyn ConfigChangeListener>,
    ) -> error::Result<()> {
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
    namespace: String,
    /// DataId
    data_id: String,
    /// Group
    group: String,
    /// Content
    content: String,
    /// Content's Type; e.g. json,properties,xml,html,text,yaml
    content_type: String,
    /// Content's md5
    md5: String,
}

impl std::fmt::Display for ConfigResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut content = self.content.clone();
        if content.chars().count() > 30 {
            content = content.chars().take(30).collect();
            content.push_str("...");
        }
        write!(
            f,
            "ConfigResponse(namespace={n},data_id={d},group={g},md5={m},content={c})",
            n = self.namespace,
            d = self.data_id,
            g = self.group,
            m = self.md5,
            c = content
        )
    }
}

impl ConfigResponse {
    pub fn new(
        data_id: String,
        group: String,
        namespace: String,
        content: String,
        content_type: String,
        md5: String,
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

    pub fn namespace(&self) -> &String {
        &self.namespace
    }
    pub fn data_id(&self) -> &String {
        &self.data_id
    }
    pub fn group(&self) -> &String {
        &self.group
    }
    pub fn content(&self) -> &String {
        &self.content
    }
    pub fn content_type(&self) -> &String {
        &self.content_type
    }
    pub fn md5(&self) -> &String {
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
/// ```ignore
///  let mut config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
///        nacos_sdk::api::props::ClientProps::new()
///           .server_addr("127.0.0.1:8848")
///           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///           .namespace("")
///           .app_name("todo-your-app-name"),
///   )
///   .build()?;
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
    pub fn build(self) -> error::Result<ConfigService> {
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
        )?;
        let inner = Arc::new(inner);
        Ok(ConfigService { inner })
    }
}

#[cfg(test)]
mod tests {
    use crate::api::config::ConfigServiceBuilder;
    use crate::api::config::{ConfigChangeListener, ConfigResponse, ConfigService};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::sleep;

    struct TestConfigChangeListener;

    impl ConfigChangeListener for TestConfigChangeListener {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::info!("listen the config={}", config_resp);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let (data_id, group) = ("test_api_config_service".to_string(), "TEST".to_string());

        let config_service = ConfigServiceBuilder::default().build().unwrap();

        // publish a config
        let _publish_resp = config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                "test_api_config_service".to_string(),
                Some("text".to_string()),
            )
            .await
            .unwrap();
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
            .unwrap();

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
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let config_service = ConfigServiceBuilder::default().build().unwrap();

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
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let config_service = ConfigServiceBuilder::default().build().unwrap();

        // publish a config
        let publish_resp = config_service
            .publish_config(
                "test_api_config_service_publish_config".to_string(),
                "TEST".to_string(),
                "test_api_config_service_publish_config".to_string(),
                Some("text".to_string()),
            )
            .await
            .unwrap();
        tracing::info!("publish a config: {}", publish_resp);
        assert_eq!(true, publish_resp);
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_publish_config_param() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let config_service = ConfigServiceBuilder::default().build().unwrap();

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
            .unwrap();
        tracing::info!("publish a config with param: {}", publish_resp);
        assert_eq!(true, publish_resp);
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_publish_config_beta() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let config_service = ConfigServiceBuilder::default().build().unwrap();

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
            .unwrap();
        tracing::info!("publish a config with beta: {}", publish_resp);
        assert_eq!(true, publish_resp);
    }

    #[tokio::test]
    #[ignore]
    async fn test_api_config_service_publish_config_cas() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let config_service = ConfigServiceBuilder::default().build().unwrap();

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
            .unwrap();
        assert_eq!(true, publish_resp);

        // sleep for config sync in server
        sleep(Duration::from_millis(111)).await;

        // get a config
        let config_resp = config_service
            .get_config(data_id.clone(), group.clone())
            .await
            .unwrap();

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
            .unwrap();
        tracing::info!("publish a config with cas: {}", publish_resp);
        assert_eq!(true, publish_resp);

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
            .unwrap();
        assert_ne!(content_cas_md5_not_right, config_resp.content().as_str());
        assert_eq!(content_cas_md5.as_str(), config_resp.content().as_str());
    }
}

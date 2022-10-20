use crate::api::{error, props};

/// Api [`ConfigService`].
///
/// # Examples
///
/// ```rust
///  let mut config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
///        nacos_sdk::api::props::ClientProps::new()
///           .server_addr("0.0.0.0:9848")
///           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///           .namespace("")
///           .app_name("todo-your-app-name"),
///   )
///   .build()//.await
///   ;
/// ```
#[doc(alias("config", "sdk", "api"))]
pub trait ConfigService {
    /// Get config, return the content.
    fn get_config(&mut self, data_id: String, group: String) -> error::Result<ConfigResponse>;

    /// Publish config, return true/false.
    fn publish_config(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> error::Result<bool>;

    /// Cas publish config with cas_md5 (prev content's md5), return true/false.
    fn publish_config_cas(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> error::Result<bool>;

    /// Beta publish config, return true/false.
    fn publish_config_beta(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> error::Result<bool>;

    /// Publish config with params (see keys [`constants::*`]), return true/false.
    fn publish_config_param(
        &mut self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: std::collections::HashMap<String, String>,
    ) -> error::Result<bool>;

    /// Remove config, return true/false.
    fn remove_config(&mut self, data_id: String, group: String) -> error::Result<bool>;

    /// Listen the config change.
    fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn ConfigChangeListener>,
    ) -> error::Result<()>;

    /// Remove a Listener.
    fn remove_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: std::sync::Arc<dyn ConfigChangeListener>,
    ) -> error::Result<()>;
}

/// The ConfigChangeListener receive a notify of [`ConfigResponse`].
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
        if self.content.len() > 30 {
            let mut content = self.content.clone();
            content.truncate(30);
            content.push_str("...");
            write!(
                f,
                "ConfigResponse(namespace={n},data_id={d},group={g},content={c})",
                n = self.namespace,
                d = self.data_id,
                g = self.group,
                c = content
            )
        } else {
            write!(
                f,
                "ConfigResponse(namespace={n},data_id={d},group={g},content={c})",
                n = self.namespace,
                d = self.data_id,
                g = self.group,
                c = self.content
            )
        }
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
    pub const KEY_PARAM_TYPE: &str = "type";

    /// param betaIps, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_BETA_IPS: &str = "betaIps";

    /// param appName, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_APP_NAME: &str = "appName";

    /// param tag, use for [`crate::api::config::ConfigService::publish_config_param`]
    pub const KEY_PARAM_TAG: &str = "tag";
}

/// Builder of api [`ConfigService`].
///
/// # Examples
///
/// ```rust
///  let mut config_service = nacos_sdk::api::config::ConfigServiceBuilder::new(
///        nacos_sdk::api::props::ClientProps::new()
///           .server_addr("0.0.0.0:9848")
///           // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
///           .namespace("")
///           .app_name("todo-your-app-name"),
///   )
///   .build()//.await
///   ;
/// ```
#[doc(alias("config", "builder"))]
pub struct ConfigServiceBuilder {
    client_props: props::ClientProps,
}

impl Default for ConfigServiceBuilder {
    fn default() -> Self {
        ConfigServiceBuilder {
            client_props: props::ClientProps::new(),
        }
    }
}

impl ConfigServiceBuilder {
    pub fn new(client_props: props::ClientProps) -> Self {
        ConfigServiceBuilder { client_props }
    }

    /// Builds a new [`ConfigService`].
    pub async fn build(self) -> impl ConfigService {
        let mut config_service = crate::config::NacosConfigService::new(self.client_props);
        config_service.start().await;
        config_service
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
            tracing::info!("listen the config={:?}", config_resp);
        }
    }

    // #[tokio::test]
    async fn test_api_config_service() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let mut config_service = ConfigServiceBuilder::default().build().await;
        let config =
            config_service.get_config("hongwen.properties".to_string(), "LOVE".to_string());
        match config {
            Ok(config) => tracing::info!("get the config {}", config),
            Err(err) => tracing::error!("get the config {:?}", err),
        }

        let _listen = config_service.add_listener(
            "hongwen.properties".to_string(),
            "LOVE".to_string(),
            std::sync::Arc::new(TestConfigChangeListener {}),
        );
        match _listen {
            Ok(_) => tracing::info!("listening the config success"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        // example get a config not exit
        let config_resp =
            config_service.get_config("todo-data-id".to_string(), "todo-group".to_string());
        match config_resp {
            Ok(config_resp) => tracing::info!("get the config {}", config_resp),
            Err(err) => tracing::error!("get the config {:?}", err),
        }

        // example add a listener with config not exit
        let _listen = config_service.add_listener(
            "todo-data-id".to_string(),
            "todo-group".to_string(),
            std::sync::Arc::new(TestConfigChangeListener {}),
        );
        match _listen {
            Ok(_) => tracing::info!("listening the config success"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        sleep(Duration::from_secs(30)).await;
    }

    // #[tokio::test]
    async fn test_api_config_service_remove_config() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let mut config_service = ConfigServiceBuilder::default().build().await;

        // remove a config not exit
        let remove_resp =
            config_service.remove_config("todo-data-id".to_string(), "todo-group".to_string());
        match remove_resp {
            Ok(result) => tracing::info!("remove a config not exit: {}", result),
            Err(err) => tracing::error!("remove a config not exit: {:?}", err),
        }
    }

    // #[tokio::test]
    async fn test_api_config_service_publish_config() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let mut config_service = ConfigServiceBuilder::default().build().await;

        // publish a config
        let publish_resp = config_service
            .publish_config(
                "test_api_config_service_publish_config".to_string(),
                "TEST".to_string(),
                "test_api_config_service_publish_config".to_string(),
                Some("text".to_string()),
            )
            .unwrap();
        tracing::info!("publish a config: {}", publish_resp);
        assert_eq!(true, publish_resp);
    }

    // #[tokio::test]
    async fn test_api_config_service_publish_config_param() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let mut config_service = ConfigServiceBuilder::default().build().await;

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
            .unwrap();
        tracing::info!("publish a config with param: {}", publish_resp);
        assert_eq!(true, publish_resp);
    }

    // #[tokio::test]
    async fn test_api_config_service_publish_config_beta() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let mut config_service = ConfigServiceBuilder::default().build().await;

        // publish a config with beta
        let publish_resp = config_service
            .publish_config_beta(
                "test_api_config_service_publish_config".to_string(),
                "TEST".to_string(),
                "test_api_config_service_publish_config_beta".to_string(),
                None,
                "127.0.0.1,192.168.0.1".to_string(),
            )
            .unwrap();
        tracing::info!("publish a config with beta: {}", publish_resp);
        assert_eq!(true, publish_resp);
    }

    // #[tokio::test]
    async fn test_api_config_service_publish_config_cas() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let mut config_service = ConfigServiceBuilder::default().build().await;

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
            .unwrap();
        assert_eq!(true, publish_resp);

        // sleep for config sync in server
        sleep(Duration::from_millis(111)).await;

        // get a config
        let config_resp = config_service
            .get_config(data_id.clone(), group.clone())
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
            .unwrap();
        tracing::info!("publish a config with cas: {}", publish_resp);
        assert_eq!(true, publish_resp);

        // publish a config with cas md5 not right
        let content_cas_md5_not_right = "test_api_config_service_publish_config_cas_md5_not_right";
        let publish_resp = config_service.publish_config_cas(
            data_id.clone(),
            group.clone(),
            content_cas_md5_not_right.to_string(),
            None,
            config_resp.md5().to_string(),
        );
        match publish_resp {
            Ok(result) => tracing::info!("publish a config with cas: {}", result),
            Err(err) => tracing::error!("publish a config with cas: {:?}", err),
        }
        sleep(Duration::from_millis(111)).await;

        let config_resp = config_service
            .get_config(data_id.clone(), group.clone())
            .unwrap();
        assert_ne!(content_cas_md5_not_right, config_resp.content().as_str());
        assert_eq!(content_cas_md5.as_str(), config_resp.content().as_str());
    }
}

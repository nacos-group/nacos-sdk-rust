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
    ) -> Self {
        ConfigResponse {
            data_id,
            group,
            namespace,
            content,
            content_type,
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
    use std::time::Duration;
    use tokio::time::sleep;

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

    struct TestConfigChangeListener;

    impl ConfigChangeListener for TestConfigChangeListener {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::info!("listen the config={:?}", config_resp);
        }
    }
}

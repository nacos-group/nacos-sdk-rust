use crate::api::{client_config, error};

pub(crate) type ConfigChangeListener = dyn Fn(ConfigResponse) + Send + Sync;

pub trait ConfigService {
    /// Get config, return the content.
    fn get_config(
        &mut self,
        data_id: String,
        group: String,
        timeout_ms: u64,
    ) -> error::Result<String>;

    /// Listen the config change.
    fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: Box<ConfigChangeListener>,
    ) -> error::Result<()>;
}

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

    pub fn get_namespace(&self) -> &String {
        &self.namespace
    }
    pub fn get_data_id(&self) -> &String {
        &self.data_id
    }
    pub fn get_group(&self) -> &String {
        &self.group
    }
    pub fn get_content(&self) -> &String {
        &self.content
    }
    pub fn get_content_type(&self) -> &String {
        &self.content_type
    }
}

pub struct ConfigServiceBuilder {
    client_config: client_config::ClientConfig,
}

impl Default for ConfigServiceBuilder {
    fn default() -> Self {
        ConfigServiceBuilder {
            client_config: client_config::ClientConfig::new(),
        }
    }
}

impl ConfigServiceBuilder {
    pub fn new(client_config: client_config::ClientConfig) -> Self {
        ConfigServiceBuilder { client_config }
    }

    /// Builds a new [`ConfigService`].
    pub async fn build(self) -> impl ConfigService {
        let mut config_service = crate::config::NacosConfigService::new(self.client_config);
        config_service.start().await;
        config_service
    }
}

#[cfg(test)]
mod tests {
    use crate::api::config::ConfigService;
    use crate::api::config::ConfigServiceBuilder;
    use std::time::Duration;
    use tokio::time::sleep;

    // #[tokio::test]
    async fn test_api_config_service() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let mut config_service = ConfigServiceBuilder::default().build().await;
        let config =
            config_service.get_config("hongwen.properties".to_string(), "LOVE".to_string(), 3000);
        match config {
            Ok(config) => tracing::info!("get the config {}", config),
            Err(err) => tracing::error!("get the config {:?}", err),
        }

        let _listen = config_service.add_listener(
            "hongwen.properties".to_string(),
            "LOVE".to_string(),
            Box::new(|config_resp| {
                tracing::info!("listen the config {}", config_resp.get_content());
            }),
        );
        match _listen {
            Ok(_) => tracing::info!("listening the config"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        sleep(Duration::from_secs(30)).await;
    }
}

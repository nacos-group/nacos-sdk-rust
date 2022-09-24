mod client_request;
mod client_response;
mod server_request;
mod server_response;
mod util;
mod worker;

use crate::api::client_config::ClientConfig;
use crate::api::config::ConfigService;
use crate::config::worker::ConfigWorker;

pub(crate) struct NacosConfigService {
    client_config: ClientConfig,
    /// config client worker
    client_worker: ConfigWorker,
}

impl NacosConfigService {
    pub fn new(client_config: ClientConfig) -> Self {
        let client_worker = ConfigWorker::new(client_config.clone());
        Self {
            client_config,
            client_worker,
        }
    }

    /// start Once
    pub(crate) async fn start(&mut self) {
        self.client_worker.start().await;
    }
}

impl ConfigService for NacosConfigService {
    fn get_config(
        &mut self,
        data_id: String,
        group: String,
        _timeout_ms: u64,
    ) -> crate::api::error::Result<crate::api::config::ConfigResponse> {
        self.client_worker.get_config(data_id, group, _timeout_ms)
    }

    fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: Box<crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker.add_listener(
            data_id.clone(),
            group.clone(),
            self.client_config.namespace.clone(),
            listener,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::api::client_config::ClientConfig;
    use crate::api::config::ConfigService;
    use crate::config::NacosConfigService;
    use std::time::Duration;
    use tokio::time::sleep;

    // #[tokio::test]
    async fn test_config_service() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let mut config_service = NacosConfigService::new(
            ClientConfig::new()
                .server_addr("0.0.0.0:9848".to_string())
                .app_name("test-app-name"),
        );
        config_service.start().await;
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
                tracing::info!("listen the config {}", config_resp.content());
            }),
        );
        match _listen {
            Ok(_) => tracing::info!("listening the config"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        sleep(Duration::from_secs(30)).await;
    }
}

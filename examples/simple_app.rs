use nacos_sdk::api::config::ConfigServiceBuilder;
use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse, ConfigService};
use nacos_sdk::api::props::ClientProps;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        // all spans/events with a level higher than TRACE (e.g, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(tracing::Level::DEBUG)
        // sets this to be the default, global collector for this application.
        .init();

    let mut config_service = ConfigServiceBuilder::new(
        ClientProps::new()
            .server_addr("0.0.0.0:9848")
            // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
            .namespace("")
            .app_name("simple_app"),
    )
    .build()?;
    let config_resp =
        config_service.get_config("hongwen.properties".to_string(), "LOVE".to_string());
    match config_resp {
        Ok(config_resp) => tracing::info!("get the config {}", config_resp),
        Err(err) => tracing::error!("get the config {:?}", err),
    }

    let _listen = config_service.add_listener(
        "hongwen.properties".to_string(),
        "LOVE".to_string(),
        std::sync::Arc::new(SimpleConfigChangeListener {}),
    );
    match _listen {
        Ok(_) => tracing::info!("listening the config success"),
        Err(err) => tracing::error!("listen config error {:?}", err),
    }

    sleep(Duration::from_secs(300)).await;

    Ok(())
}

struct SimpleConfigChangeListener;

impl ConfigChangeListener for SimpleConfigChangeListener {
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("listen the config={}", config_resp);
    }
}

use nacos_client::api::config::ConfigService;
use nacos_client::api::config::ConfigServiceBuilder;
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
            tracing::info!("listen the config={:?}", config_resp);
        }),
    );
    match _listen {
        Ok(_) => tracing::info!("listening the config"),
        Err(err) => tracing::error!("listen config error {:?}", err),
    }

    sleep(Duration::from_secs(300)).await;

    Ok(())
}

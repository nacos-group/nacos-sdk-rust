use nacos_client::api::config::ConfigService;
use nacos_client::api::config::ConfigServiceBuilder;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

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
    tracing::info!("get the config {}", config.expect("None"));

    let _listen = config_service.listen(
        "hongwen.properties".to_string(),
        "LOVE".to_string(),
        Arc::new(|config_resp| {
            tracing::info!("listen the config {}", config_resp.get_content());
        }),
    );

    sleep(Duration::from_secs(300));

    Ok(())
}

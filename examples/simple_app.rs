use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::constants;
use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingService, NamingServiceBuilder, ServiceInstance,
};
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

    let client_props = ClientProps::new()
        .server_addr("0.0.0.0:8848")
        // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
        .namespace("")
        .app_name("simple_app")
        // .auth_username("TODO")
        // .auth_password("TODO")
        ;

    // ----------  Config  -------------
    let mut config_service = ConfigServiceBuilder::new(client_props.clone())
        // .enable_auth_plugin_http()
        .build()?;
    let config_resp = config_service.get_config("todo-data-id".to_string(), "LOVE".to_string());
    match config_resp {
        Ok(config_resp) => tracing::info!("get the config {}", config_resp),
        Err(err) => tracing::error!("get the config {:?}", err),
    }

    let _listen = config_service.add_listener(
        "todo-data-id".to_string(),
        "LOVE".to_string(),
        std::sync::Arc::new(SimpleConfigChangeListener {}),
    );
    match _listen {
        Ok(_) => tracing::info!("listening the config success"),
        Err(err) => tracing::error!("listen config error {:?}", err),
    }

    // ----------  Naming  -------------
    let mut naming_service = NamingServiceBuilder::new(client_props)
        // .enable_auth_plugin_http()
        .build()?;

    let listener = std::sync::Arc::new(SimpleInstanceChangeListener);
    let _subscribe_ret = naming_service.subscribe(
        "test-service".to_string(),
        Some(constants::DEFAULT_GROUP.to_string()),
        Vec::default(),
        listener,
    );

    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        ..Default::default()
    };
    let _register_instance_ret = naming_service.batch_register_instance(
        "test-service".to_string(),
        Some(constants::DEFAULT_GROUP.to_string()),
        vec![service_instance1],
    );
    sleep(Duration::from_millis(111)).await;

    let instances_ret = naming_service.get_all_instances(
        "test-service".to_string(),
        Some(constants::DEFAULT_GROUP.to_string()),
        Vec::default(),
        false,
    );
    match instances_ret {
        Ok(instances) => tracing::info!("get_all_instances {:?}", instances),
        Err(err) => tracing::error!("naming get_all_instances error {:?}", err),
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

pub struct SimpleInstanceChangeListener;

impl NamingEventListener for SimpleInstanceChangeListener {
    fn event(&self, event: std::sync::Arc<NamingChangeEvent>) {
        tracing::info!("subscriber notify: {:?}", event);
    }
}

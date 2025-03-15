use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::constants;
use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingService, NamingServiceBuilder, ServiceInstance,
};
use nacos_sdk::api::props::ClientProps;

use std::sync::LazyLock;

static CLIENT_PROPS: LazyLock<ClientProps> = LazyLock::new(|| {
    ClientProps::new()
        .server_addr(constants::DEFAULT_SERVER_ADDR)
        // .remote_grpc_port(9838)
        // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
        .namespace("")
        .app_name("lazy_app")
        .auth_username("nacos") // TODO You can choose not to enable auth
        .auth_password("nacos") // TODO You can choose not to enable auth
});

// 请注意！一般情况下，应用下仅需一个 Config 客户端，而且需要长期持有直至应用停止。
// 因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。
static CONFIG_SERVICE: LazyLock<ConfigService> = LazyLock::new(|| {
    let config_service = ConfigServiceBuilder::new(CLIENT_PROPS.clone())
        .enable_auth_plugin_http() // TODO You can choose not to enable auth
        .build()
        .unwrap();
    config_service
});

// 请注意！一般情况下，应用下仅需一个 Naming 客户端，而且需要长期持有直至应用停止。
// 因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。
static NAMING_SERVICE: LazyLock<NamingService> = LazyLock::new(|| {
    let naming_service = NamingServiceBuilder::new(CLIENT_PROPS.clone())
        .enable_auth_plugin_http() // TODO You can choose not to enable auth
        .build()
        .unwrap();
    naming_service
});

/// enable https auth run with command:
/// cargo run --example lazy_app --features default,tls
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        // all spans/events with a level higher than TRACE (e.g, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(tracing::Level::INFO)
        .with_thread_names(true)
        .with_thread_ids(true)
        // sets this to be the default, global collector for this application.
        .init();

    // ----------  Config  -------------
    let config_resp = CONFIG_SERVICE
        .get_config("todo-data-id".to_string(), "LOVE".to_string())
        .await;
    match config_resp {
        Ok(config_resp) => tracing::info!("get the config {}", config_resp),
        Err(err) => tracing::error!("get the config {:?}", err),
    }

    let _listen = CONFIG_SERVICE
        .add_listener(
            "todo-data-id".to_string(),
            "LOVE".to_string(),
            std::sync::Arc::new(SimpleConfigChangeListener {}),
        )
        .await;
    match _listen {
        Ok(_) => tracing::info!("listening the config success"),
        Err(err) => tracing::error!("listen config error {:?}", err),
    }

    // ----------  Naming  -------------
    let listener = std::sync::Arc::new(SimpleInstanceChangeListener);
    let _subscribe_ret = NAMING_SERVICE
        .subscribe(
            "test-service".to_string(),
            Some(constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            listener,
        )
        .await;

    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        ..Default::default()
    };
    let _register_instance_ret = NAMING_SERVICE
        .batch_register_instance(
            "test-service".to_string(),
            Some(constants::DEFAULT_GROUP.to_string()),
            vec![service_instance1],
        )
        .await;
    tokio::time::sleep(tokio::time::Duration::from_millis(666)).await;

    let instances_ret = NAMING_SERVICE
        .get_all_instances(
            "test-service".to_string(),
            Some(constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            false,
        )
        .await;
    match instances_ret {
        Ok(instances) => tracing::info!("get_all_instances {:?}", instances),
        Err(err) => tracing::error!("naming get_all_instances error {:?}", err),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;

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

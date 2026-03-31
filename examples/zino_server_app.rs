//! Zino web server integrated with Nacos for config management and service registration.
//!
//! ## Architecture
//!
//! This example demonstrates a zino-based HTTP server that:
//! - Reads configuration from Nacos (e.g. greeting message)
//! - Registers itself as a service instance in Nacos for discovery
//! - Listens for config changes in real-time via Nacos long-lived connection
//!
//! ## Usage
//!
//! 1. Start a local Nacos server:
//!    ```bash
//!    docker run --name nacos-quick -e MODE=standalone -p 8848:8848 -p 9848:9848 -d nacos/nacos-server:v2.5.2
//!    ```
//!
//! 2. Create config in Nacos console:
//!    - Data ID: `greeting`
//!    - Group: `SERVER_APP_GROUP`
//!    - Content: `Hello from Nacos!`
//!
//! 3. Run the server (default port 6080, or set ZINO_MAIN_PORT env var):
//!    ```bash
//!    cargo run --example zino_server_app
//!    # or with custom port:
//!    ZINO_MAIN_PORT=8000 cargo run --example zino_server_app
//!    ```
//!
//! 4. Test:
//!    ```bash
//!    curl http://127.0.0.1:6080/greeting
//!    curl http://127.0.0.1:6080/health
//!    ```
//!
//! ## Key Design
//!
//! - 请注意！一般情况下，应用下仅需一个 Config/Naming 客户端，而且需要长期持有直至应用停止。
//!   因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。
//! - Nacos 客户端通过 OnceCell 懒初始化，在首次路由调用时创建并订阅/注册。
//! - 服务实例为 ephemeral=true（默认），当进程退出 gRPC 长链接断开时，Nacos 会自动清理。
//! - zino 框架自身管理 Ctrl+C 信号处理和 HTTP 服务器优雅关闭。

use axum::{Router, routing::get};
use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::constants;
use nacos_sdk::api::naming::{NamingService, NamingServiceBuilder, ServiceInstance};
use nacos_sdk::api::props::ClientProps;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::sync::OnceCell;
use zino::{Cluster, Request, Response, Result, prelude::*};

static LOCAL_IP: LazyLock<String> =
    LazyLock::new(|| local_ipaddress::get().unwrap_or_else(|| "127.0.0.1".to_string()));

static CLIENT_PROPS: LazyLock<ClientProps> = LazyLock::new(|| {
    ClientProps::new()
        .server_addr(constants::DEFAULT_SERVER_ADDR)
        .namespace("")
        .app_name("zino_server_app")
        .auth_username("nacos")
        .auth_password("nacos")
});

static CONFIG_SERVICE: OnceCell<ConfigService> = OnceCell::const_new();
static NAMING_SERVICE: OnceCell<NamingService> = OnceCell::const_new();

const DEFAULT_SERVER_PORT: i32 = 6080;
const SERVICE_NAME: &str = "zino-server-app";
const CONFIG_DATA_ID: &str = "greeting";
const CONFIG_GROUP: &str = "SERVER_APP_GROUP";

fn server_port() -> i32 {
    std::env::var("ZINO_MAIN_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SERVER_PORT)
}

// ── Lazy getters ──────────────────────────────────────────────────

async fn get_config_service() -> &'static ConfigService {
    CONFIG_SERVICE
        .get_or_init(|| async {
            let service = ConfigServiceBuilder::new((*CLIENT_PROPS).clone())
                .enable_auth_plugin_http()
                .build()
                .await
                .expect("Failed to build ConfigService");
            let _ = service
                .add_listener(
                    CONFIG_DATA_ID.to_string(),
                    CONFIG_GROUP.to_string(),
                    Arc::new(ServerConfigChangeListener),
                )
                .await;
            tracing::info!("ConfigService initialized and listener registered");
            service
        })
        .await
}

async fn get_naming_service() -> &'static NamingService {
    NAMING_SERVICE
        .get_or_init(|| async {
            let port = server_port();
            let service = NamingServiceBuilder::new((*CLIENT_PROPS).clone())
                .enable_auth_plugin_http()
                .build()
                .await
                .expect("Failed to build NamingService");
            let instance = ServiceInstance {
                ip: LOCAL_IP.to_string(),
                port,
                ..Default::default()
            };
            let _ = service
                .batch_register_instance(
                    SERVICE_NAME.to_string(),
                    Some(constants::DEFAULT_GROUP.to_string()),
                    vec![instance],
                )
                .await;
            tracing::info!(
                "NamingService initialized and registered: {}:{port}",
                *LOCAL_IP
            );
            service
        })
        .await
}

// ── Handlers ──────────────────────────────────────────────────────

async fn greeting(req: Request) -> Result {
    let config_service = get_config_service().await;
    let greeting_text = match config_service
        .get_config(CONFIG_DATA_ID.to_string(), CONFIG_GROUP.to_string())
        .await
    {
        Ok(resp) => resp.content().clone(),
        Err(err) => {
            tracing::warn!("Failed to get config from Nacos: {:?}", err);
            "Hello from Zino+Nacos!".to_string()
        }
    };

    let port = server_port();
    let mut res = Response::ok().context(&req);
    res.set_json_data(json!({
        "greeting": greeting_text,
        "server": SERVICE_NAME,
        "ip": *LOCAL_IP,
        "port": port,
    }));
    Ok(res.into())
}

async fn health(req: Request) -> Result {
    // Ensure naming service is initialized (triggers registration)
    let _ = get_naming_service().await;
    let port = server_port();
    let mut res = Response::ok().context(&req);
    res.set_json_data(json!({
        "status": "UP",
        "server": SERVICE_NAME,
        "ip": *LOCAL_IP,
        "port": port,
    }));
    Ok(res.into())
}

fn routes() -> Vec<Router> {
    vec![
        Router::new()
            .route("/greeting", get(greeting))
            .route("/health", get(health)),
    ]
}

// ── Main ──────────────────────────────────────────────────────────

/// enable tls run with command:
/// cargo run --example zino_server_app --features default,tls
fn main() {
    let port = server_port();

    // Zino reads port from config file; create a temporary one if it doesn't exist.
    // ZINO_MAIN_PORT env var only overrides if the config file already has [main].port.
    std::fs::create_dir_all("config").ok();
    std::fs::write(
        "config/config.dev.toml",
        format!("[main]\nhost = \"127.0.0.1\"\nport = {port}\n"),
    )
    .expect("Failed to write config file");

    println!("Starting zino-server-app on port {port}");

    Cluster::boot().register(routes()).run();
}

struct ServerConfigChangeListener;

impl ConfigChangeListener for ServerConfigChangeListener {
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("Config changed: {}", config_resp);
    }
}

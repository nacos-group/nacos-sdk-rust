//! Zino web client that discovers and calls the server via Nacos service discovery.
//!
//! ## Architecture
//!
//! This example demonstrates a zino-based HTTP client that:
//! - Discovers the server (zino-server-app) via Nacos service discovery
//! - Subscribes to service instance changes in real-time
//! - Calls the server's endpoints using HTTP
//!
//! ## Usage
//!
//! 1. Make sure zino_server_app is running first (see zino_server_app.rs).
//!
//! 2. Run the client (default port 6081, or set ZINO_MAIN_PORT env var):
//!    ```bash
//!    cargo run --example zino_client_app
//!    # or with custom port:
//!    ZINO_MAIN_PORT=8001 cargo run --example zino_client_app
//!    ```
//!
//! 3. Test:
//!    ```bash
//!    curl http://127.0.0.1:6081/call-greeting
//!    curl http://127.0.0.1:6081/call-health
//!    ```
//!
//! ## Key Design
//!
//! - 请注意！一般情况下，应用下仅需一个 Naming 客户端，而且需要长期持有直至应用停止。
//!   因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。
//! - Naming 客户端通过 zino **Plugin 机制在启动阶段** 完成初始化与服务订阅，确保 HTTP 服务开始接受请求前，Nacos 长连接已就绪。
//! - 服务发现后使用 reqwest 发起 HTTP 调用（plain HTTP，适用于本地开发场景）。

use axum::{Router, routing::get};
use nacos_sdk::api::constants;
use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingService, NamingServiceBuilder,
};
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
        .namespace("zino-test")
        .app_name("zino_client_app")
        .auth_username("nacos")
        .auth_password("nacos")
        .load_cache_at_start(true)
});

static NAMING_SERVICE: OnceCell<NamingService> = OnceCell::const_new();

const DEFAULT_CLIENT_PORT: i32 = 6081;
const SERVICE_NAME: &str = "zino-server-app";

fn client_port() -> i32 {
    std::env::var("ZINO_MAIN_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CLIENT_PORT)
}

// ── Nacos Plugin ──────────────────────────────────────────────────

fn nacos_plugin() -> Plugin {
    let loader = Box::pin(async {
        let service = NamingServiceBuilder::new((*CLIENT_PROPS).clone())
            .enable_auth_plugin_http()
            .build()
            .await
            .expect("Failed to build NamingService");
        let _ = service
            .subscribe(
                SERVICE_NAME.to_string(),
                Some(constants::DEFAULT_GROUP.to_string()),
                Vec::default(),
                Arc::new(ClientInstanceChangeListener),
            )
            .await;
        NAMING_SERVICE
            .set(service)
            .expect("NamingService already initialized");
        tracing::info!("Nacos plugin ready: subscribed to {SERVICE_NAME}");
        Ok(())
    });
    Plugin::with_loader("nacos", loader)
}

// ── Handlers ──────────────────────────────────────────────────────

async fn call_server(path: &str) -> std::result::Result<String, String> {
    let naming_service = NAMING_SERVICE.get().expect("NamingService not initialized");

    let instance = naming_service
        .select_one_healthy_instance(
            SERVICE_NAME.to_string(),
            Some(constants::DEFAULT_GROUP.to_string()),
            Vec::default(),
            true,
        )
        .await
        .map_err(|e| format!("Service discovery failed: {e:?}"))?;

    let url = format!("http://{}:{}{}", instance.ip, instance.port, path);
    tracing::info!("Calling server at {url}");

    let body = reqwest::get(&url)
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    Ok(body)
}

async fn call_greeting(req: Request) -> Result {
    let port = client_port();
    let res = match call_server("/greeting").await {
        Ok(body) => {
            let mut res = Response::ok().context(&req);
            #[allow(clippy::disallowed_methods)]
            res.set_json_data(json!({
                "client": { "ip": *LOCAL_IP, "port": port },
                "server_response": body,
            }));
            res
        }
        Err(err) => {
            tracing::error!("call greeting failed: {err}");
            let mut res = Response::service_unavailable().context(&req);
            #[allow(clippy::disallowed_methods)]
            res.set_json_data(json!({
                "client": { "ip": *LOCAL_IP, "port": port },
                "error": err,
            }));
            res
        }
    };
    Ok(res.into())
}

async fn call_health(req: Request) -> Result {
    let port = client_port();
    let res = match call_server("/health").await {
        Ok(body) => {
            let mut res = Response::ok().context(&req);
            #[allow(clippy::disallowed_methods)]
            res.set_json_data(json!({
                "client": { "ip": *LOCAL_IP, "port": port },
                "server_response": body,
            }));
            res
        }
        Err(err) => {
            tracing::error!("call health failed: {err}");
            let mut res = Response::service_unavailable().context(&req);
            #[allow(clippy::disallowed_methods)]
            res.set_json_data(json!({
                "client": { "ip": *LOCAL_IP, "port": port },
                "error": err,
            }));
            res
        }
    };
    Ok(res.into())
}

fn routes() -> Vec<Router> {
    vec![
        Router::new()
            .route("/call-greeting", get(call_greeting))
            .route("/call-health", get(call_health)),
    ]
}

// ── Main ──────────────────────────────────────────────────────────

/// enable tls run with command:
/// cargo run --example zino_client_app --features default,tls
fn main() {
    let port = client_port();

    // Zino reads port from config file; create a temporary one if it doesn't exist.
    // ZINO_MAIN_PORT env var only overrides if the config file already has [main].port.
    std::fs::create_dir_all("config").ok();
    std::fs::write(
        "config/config.dev.toml",
        format!("[main]\nhost = \"0.0.0.0\"\nport = {port}\n"),
    )
    .expect("Failed to write config file");

    println!("Starting zino-client-app on port {port}");

    Cluster::boot()
        .add_plugin(nacos_plugin()) // ← Nacos init + subscription BEFORE HTTP bind
        .register(routes())
        .run();
}

struct ClientInstanceChangeListener;

impl NamingEventListener for ClientInstanceChangeListener {
    fn event(&self, event: Arc<NamingChangeEvent>) {
        tracing::info!(
            "Service instances changed for {}: {:?}",
            event.service_name,
            event.instances
        );
    }
}

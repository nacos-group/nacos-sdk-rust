use nacos_sdk::api::config::ConfigServiceBuilder;
use nacos_sdk::api::naming::{NamingServiceBuilder, ServiceInstance};
use nacos_sdk::api::props::ClientProps;
use std::time::Duration;
use tokio::time::sleep;

/// Aliyun Ram plugin support
///
/// Notice:
/// accessKey and secretKey are sensitive data, don't encode them in you code
/// directly, inject it via environment variables are recommended.
///
/// Example run preparations:
/// 1. inject you accessKey and secretKey via environment variables by following command
/// export NACOS_CLIENT_ACCESS_KEY=you_access_key
/// export NACOS_CLIENT_SECRET_KEY=you_secret_key
///
/// 2. run command
/// cargo run --example aliyun_ram_app --features default,auth-by-aliyun

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "auth-by-aliyun")]
    run_config_demo().await;

    #[cfg(feature = "auth-by-aliyun")]
    run_naming_demo().await;
    Ok(())
}

#[cfg(feature = "auth-by-aliyun")]
async fn run_naming_demo() {
    let server_addr = "localhost:8848";

    /// NamingService
    let naming_client = NamingServiceBuilder::new(ClientProps::new().server_addr(server_addr))
        .enable_auth_plugin_aliyun()
        .build()
        .unwrap();

    let mut instance = ServiceInstance::default();
    instance.ip = "localhost".to_string();
    instance.port = 8080;

    println!("Register localhost:8080 to service(name: test, group: test)");
    naming_client
        .register_instance("test".to_owned(), Some("test".to_owned()), instance)
        .await
        .unwrap();

    println!("Get All instance from service(name:test, group: test)");
    let instances = naming_client
        .get_all_instances(
            "test".to_string(),
            Some("test".to_string()),
            Vec::new(),
            false,
        )
        .await
        .unwrap();
    assert_eq!(1, instances.len());
    assert_eq!("localhost", instances[0].ip);
    assert_eq!(8080, instances[0].port);
}

#[cfg(feature = "auth-by-aliyun")]
async fn run_config_demo() {
    let server_addr = "localhost:8848";

    /// Config service
    let config_client = ConfigServiceBuilder::new(ClientProps::new().server_addr(server_addr))
        .enable_auth_plugin_aliyun()
        .build()
        .unwrap();

    println!(
        "Publish config dataId = {}, group = {}, content = {}",
        "test", "test", "test=test"
    );
    config_client
        .publish_config(
            "test".to_string(),
            "test".to_string(),
            "test=test".to_string(),
            Some("properties".to_string()),
        )
        .await
        .unwrap();

    println!("Waiting...");
    sleep(Duration::from_secs(5)).await;

    let response = config_client
        .get_config("test".to_string(), "test".to_string())
        .await
        .unwrap();
    println!(
        "Get config from nacos for dataId = {}, group = {}, content = {}",
        "test",
        "test",
        response.content()
    );
    assert_eq!("test=test", response.content());
    assert_eq!("properties", response.content_type());
    assert_eq!("test", response.group());
    assert_eq!("test", response.data_id());
}

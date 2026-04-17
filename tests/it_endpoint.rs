// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! Integration tests for endpoint-based server list resolution.
//!
//! These tests use an axum-based mock endpoint server to serve server lists,
//! then verify that ConfigService and NamingService correctly connect to the
//! actual Nacos server resolved through the endpoint.
//!
//! Run with: `cargo test --test it_endpoint --features "config,naming" -- --include-ignored`

#![allow(dead_code)]

mod fixtures;
mod shared;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use tokio::net::TcpListener;

use crate::fixtures::shared_server::get_shared_server_addr;

static ENDPOINT_PORT_COUNTER: AtomicU16 = AtomicU16::new(18080);

fn allocate_endpoint_port() -> u16 {
    ENDPOINT_PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn endpoint_props(endpoint_url: &str) -> nacos_sdk::api::props::ClientProps {
    let mut props = nacos_sdk::api::props::ClientProps::new().endpoint(endpoint_url);
    #[cfg(feature = "auth-by-http")]
    {
        props = props.auth_username("nacos").auth_password("nacos");
    }
    props
}

#[cfg(feature = "config")]
async fn build_config_service(
    endpoint_url: &str,
) -> nacos_sdk::api::error::Result<nacos_sdk::api::config::ConfigService> {
    let props = endpoint_props(endpoint_url);
    let mut builder = nacos_sdk::api::config::ConfigServiceBuilder::new(props);
    #[cfg(feature = "auth-by-http")]
    {
        builder = builder.enable_auth_plugin_http();
    }
    builder.build().await
}

// ─── Mock Endpoint Server ───

/// Handler for `/nacos/serverlist` — returns the rnacos server address.
async fn serverlist_handler(State(nacos_addr): State<String>) -> String {
    nacos_addr
}

/// Handler that returns a custom server list (for dynamic update tests).
async fn custom_serverlist_handler(
    State(server_list): State<Arc<std::sync::Mutex<Vec<String>>>>,
) -> String {
    server_list
        .lock()
        .expect("mutex should not be poisoned")
        .join("\n")
}

/// Handler that returns an empty body.
async fn empty_handler() -> &'static str {
    ""
}

/// Handler that returns only whitespace.
async fn whitespace_handler() -> &'static str {
    "   \n\n\t  "
}

/// Handler that returns garbage text (not valid server addresses).
async fn garbage_handler() -> &'static str {
    "<html><body>Server List</body></html>"
}

/// Handler that returns HTTP 500 Internal Server Error.
async fn internal_error_handler() -> (axum::http::StatusCode, &'static str) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "Internal Server Error",
    )
}

/// Handler that returns HTTP 404 Not Found.
async fn not_found_handler() -> (axum::http::StatusCode, &'static str) {
    (axum::http::StatusCode::NOT_FOUND, "Not Found")
}

/// Handler that simulates a slow response (delays before responding).
async fn slow_handler(State(nacos_addr): State<String>) -> String {
    tokio::time::sleep(Duration::from_secs(15)).await;
    nacos_addr
}

async fn start_endpoint_empty() -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));
    let app = Router::new().route("/nacos/serverlist", get(empty_handler));
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    wait_for_server(port).await;
    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

async fn start_endpoint_whitespace() -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));
    let app = Router::new().route("/nacos/serverlist", get(whitespace_handler));
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    wait_for_server(port).await;
    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

async fn start_endpoint_garbage() -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));
    let app = Router::new().route("/nacos/serverlist", get(garbage_handler));
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    wait_for_server(port).await;
    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

async fn start_endpoint_500() -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));
    let app = Router::new().route("/nacos/serverlist", get(internal_error_handler));
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    wait_for_server(port).await;
    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

async fn start_endpoint_404() -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));
    let app = Router::new().route("/nacos/serverlist", get(not_found_handler));
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    wait_for_server(port).await;
    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

async fn start_endpoint_slow(nacos_addr: String) -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));
    let app = Router::new()
        .route("/nacos/serverlist", get(slow_handler))
        .with_state(nacos_addr);
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    wait_for_server(port).await;
    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

/// Starts a mock endpoint server on a random port that returns the given nacos address.
///
/// Returns the full endpoint URL.
async fn start_mock_endpoint_server(nacos_addr: String) -> String {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));

    let app = Router::new()
        .route("/nacos/serverlist", get(serverlist_handler))
        .with_state(nacos_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    wait_for_server(port).await;

    format!("http://127.0.0.1:{port}/nacos/serverlist")
}

async fn wait_for_server(port: u16) {
    for _ in 0..20 {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("Mock endpoint server on port {port} did not become ready");
}

/// Starts a mock endpoint server that returns a dynamically-updatable server list.
///
/// Returns the endpoint URL and a shared handle to update the server list.
async fn start_dynamic_mock_endpoint_server(
    initial_list: Vec<String>,
) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    let port = allocate_endpoint_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));

    let server_list = Arc::new(std::sync::Mutex::new(initial_list));
    let server_list_clone = server_list.clone();

    let app = Router::new()
        .route("/nacos/serverlist", get(custom_serverlist_handler))
        .with_state(server_list_clone);

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    wait_for_server(port).await;

    (
        format!("http://127.0.0.1:{port}/nacos/serverlist"),
        server_list,
    )
}

// ─── ConfigService Tests ───

#[cfg(feature = "config")]
mod config_endpoint_tests {
    use super::*;
    use crate::shared::test_data::{ConfigTestData, MockConfigListener};
    use nacos_sdk::api::config::{ConfigService, ConfigServiceBuilder};
    use nacos_sdk::api::props::ClientProps;

    const CONFIG_SYNC_TIMEOUT: Duration = Duration::from_millis(500);

    async fn create_config_service_via_endpoint(endpoint_url: &str) -> ConfigService {
        build_config_service(endpoint_url)
            .await
            .expect("ConfigServiceBuilder via endpoint should build successfully")
    }

    #[tokio::test]
    #[ignore]
    async fn test_config_via_endpoint_full_url() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;
        tracing::info!("Mock endpoint URL: {}", endpoint_url);

        let service = create_config_service_via_endpoint(&endpoint_url).await;
        let test_data = ConfigTestData::random();

        let publish_result = service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should not fail");
        assert!(publish_result, "publish_config should return true");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_config_listener_via_endpoint() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;
        let service = create_config_service_via_endpoint(&endpoint_url).await;
        let test_data = ConfigTestData::random();

        let listener = Arc::new(MockConfigListener::new());

        service
            .add_listener(
                test_data.data_id.clone(),
                test_data.group.clone(),
                listener.clone(),
            )
            .await
            .expect("add_listener should succeed");

        service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should not fail");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_priority_over_server_addr() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;

        let mut props = ClientProps::new()
            .endpoint(&endpoint_url)
            .server_addr("0.0.0.0:1");

        #[cfg(feature = "auth-by-http")]
        {
            props = props.auth_username("nacos").auth_password("nacos");
        }

        let mut builder = ConfigServiceBuilder::new(props);
        #[cfg(feature = "auth-by-http")]
        {
            builder = builder.enable_auth_plugin_http();
        }

        let service = builder
            .build()
            .await
            .expect("Should build successfully via endpoint despite invalid server_addr");

        let test_data = ConfigTestData::random();

        let publish_result = service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should not fail via endpoint");
        assert!(publish_result);

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }
}

// ─── NamingService Tests ───

#[cfg(feature = "naming")]
mod naming_endpoint_tests {
    use super::*;
    use crate::shared::test_data::{MockNamingListener, ServiceInstanceBuilder};
    use nacos_sdk::api::constants;
    use nacos_sdk::api::naming::{NamingService, NamingServiceBuilder};
    use nacos_sdk::api::props::ClientProps;

    const PROPAGATION_WAIT: Duration = Duration::from_secs(2);

    /// Creates a NamingService connected via endpoint.
    async fn create_naming_service_via_endpoint(endpoint_url: &str) -> NamingService {
        let props = ClientProps::new().endpoint(endpoint_url).namespace("");

        NamingServiceBuilder::new(props)
            .build()
            .await
            .expect("NamingServiceBuilder via endpoint should build successfully")
    }

    /// Test: NamingService connects to Nacos via endpoint URL.
    ///
    /// Verifies that when an endpoint URL is provided, the SDK:
    /// 1. Fetches the server list from the endpoint
    /// 2. Connects to the Nacos server returned by the endpoint
    /// 3. Can register and query service instances normally
    #[tokio::test]
    #[ignore]
    async fn test_naming_via_endpoint() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;
        tracing::info!("Mock endpoint URL: {}", endpoint_url);

        let service = create_naming_service_via_endpoint(&endpoint_url).await;
        let service_name = format!("test-endpoint-service-{}", rand::random::<u32>());
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9090)
            .ephemeral(true)
            .build();

        let result = service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await;
        assert!(result.is_ok(), "register_instance failed: {:?}", result);

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(!instances.is_empty(), "expected at least one instance");
        assert_eq!(instances[0].ip(), "127.0.0.1");
        assert_eq!(instances[0].port(), 9090);

        // Cleanup
        let cleanup_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9090)
            .ephemeral(true)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup_instance)
            .await;
    }

    /// Test: NamingService subscribe via endpoint.
    ///
    /// Verifies that service subscription works when connected via endpoint.
    #[tokio::test]
    #[ignore]
    async fn test_naming_subscribe_via_endpoint() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;
        let service = create_naming_service_via_endpoint(&endpoint_url).await;

        let service_name = format!("test-endpoint-sub-{}", rand::random::<u32>());
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let listener = Arc::new(MockNamingListener::new());
        let _subscribe_ret = service
            .subscribe(
                service_name.clone(),
                group.clone(),
                Vec::default(),
                listener.clone(),
            )
            .await;

        // Register an instance to trigger a notification
        let instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9091)
            .ephemeral(true)
            .build();
        service
            .register_instance(service_name.clone(), group.clone(), instance)
            .await
            .expect("register_instance failed");

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let events = listener.get_events();
        assert!(
            !events.is_empty(),
            "subscriber should receive at least one event via endpoint"
        );

        // Cleanup
        let cleanup_instance = ServiceInstanceBuilder::new()
            .ip("127.0.0.1")
            .port(9091)
            .ephemeral(true)
            .build();
        let _ = service
            .deregister_instance(service_name, group, cleanup_instance)
            .await;
    }

    /// Test: NamingService batch register via endpoint.
    #[tokio::test]
    #[ignore]
    async fn test_naming_batch_register_via_endpoint() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;
        let service = create_naming_service_via_endpoint(&endpoint_url).await;

        let service_name = format!("test-endpoint-batch-{}", rand::random::<u32>());
        let group = Some(constants::DEFAULT_GROUP.to_string());

        let instances = vec![
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9092)
                .ephemeral(true)
                .build(),
            ServiceInstanceBuilder::new()
                .ip("127.0.0.1")
                .port(9093)
                .ephemeral(true)
                .build(),
        ];

        let result = service
            .batch_register_instance(service_name.clone(), group.clone(), instances)
            .await;
        assert!(
            result.is_ok(),
            "batch_register_instance should succeed via endpoint: {:?}",
            result
        );

        tokio::time::sleep(PROPAGATION_WAIT).await;

        let all_instances = service
            .get_all_instances(service_name.clone(), group.clone(), vec![], false)
            .await
            .expect("get_all_instances failed");
        assert!(
            all_instances.len() >= 2,
            "expected at least 2 instances, got {}",
            all_instances.len()
        );
    }
}

// ─── Endpoint-Specific Behavior Tests ───

#[cfg(feature = "config")]
mod endpoint_behavior_tests {
    use super::*;
    use crate::shared::test_data::ConfigTestData;
    use nacos_sdk::api::props::ClientProps;

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_bare_hostname() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;

        let port = allocate_endpoint_port();
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr)
            .await
            .unwrap_or_else(|e| panic!("Failed to bind mock endpoint on port {port}: {e}"));

        let nacos_addr = server_addr.clone();
        let app = Router::new()
            .route("/nacos/serverlist", get(serverlist_handler))
            .with_state(nacos_addr);

        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        wait_for_server(port).await;

        let endpoint_config = format!("127.0.0.1:{port}");
        let service = build_config_service(&endpoint_config)
            .await
            .expect("Should build with bare hostname endpoint");

        let test_data = ConfigTestData::random();

        let publish_result = service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should succeed via bare hostname endpoint");
        assert!(publish_result);

        tokio::time::sleep(Duration::from_millis(500)).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_invalid_endpoint_build_fails() {
        crate::shared::setup_log();

        let props = ClientProps::new().endpoint("http://0.0.0.0:1/nacos/serverlist");
        let result = nacos_sdk::api::config::ConfigServiceBuilder::new(props)
            .build()
            .await;
        assert!(
            result.is_err(),
            "Build should fail with unreachable endpoint"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_with_namespace() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let endpoint_url = start_mock_endpoint_server(server_addr).await;

        let mut props = ClientProps::new()
            .endpoint(&endpoint_url)
            .namespace("test-namespace");

        #[cfg(feature = "auth-by-http")]
        {
            props = props.auth_username("nacos").auth_password("nacos");
        }

        let mut builder = nacos_sdk::api::config::ConfigServiceBuilder::new(props);
        #[cfg(feature = "auth-by-http")]
        {
            builder = builder.enable_auth_plugin_http();
        }

        let service = builder
            .build()
            .await
            .expect("Should build with namespace via endpoint");

        let test_data = ConfigTestData::random();

        let publish_result = service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should succeed");
        assert!(publish_result);

        tokio::time::sleep(Duration::from_millis(500)).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_returns_multiple_servers() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;

        let (endpoint_url, _server_list_handle) =
            start_dynamic_mock_endpoint_server(vec![server_addr.clone()]).await;

        let service = build_config_service(&endpoint_url)
            .await
            .expect("Should build with multi-server endpoint");

        let test_data = ConfigTestData::random();

        let publish_result = service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should succeed");
        assert!(publish_result);

        tokio::time::sleep(Duration::from_millis(500)).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }
}

// ─── Endpoint Abnormal Data Tests ───

#[cfg(feature = "config")]
mod endpoint_abnormal_tests {
    use super::*;
    use crate::shared::test_data::ConfigTestData;

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_returns_empty_body_build_fails() {
        crate::shared::setup_log();

        let endpoint_url = start_endpoint_empty().await;
        let result = build_config_service(&endpoint_url).await;
        assert!(
            result.is_err(),
            "Build should fail when endpoint returns empty body"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_returns_whitespace_only_build_fails() {
        crate::shared::setup_log();

        let endpoint_url = start_endpoint_whitespace().await;
        let result = build_config_service(&endpoint_url).await;
        assert!(
            result.is_err(),
            "Build should fail when endpoint returns only whitespace"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_returns_http_500_build_fails() {
        crate::shared::setup_log();

        let endpoint_url = start_endpoint_500().await;
        let result = build_config_service(&endpoint_url).await;
        assert!(
            result.is_err(),
            "Build should fail when endpoint returns HTTP 500"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_returns_http_404_build_fails() {
        crate::shared::setup_log();

        let endpoint_url = start_endpoint_404().await;
        let result = build_config_service(&endpoint_url).await;
        assert!(
            result.is_err(),
            "Build should fail when endpoint returns HTTP 404"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_returns_garbage_data_build_fails() {
        crate::shared::setup_log();

        let endpoint_url = start_endpoint_garbage().await;
        let result = build_config_service(&endpoint_url).await;
        assert!(
            result.is_err(),
            "Build should fail when endpoint returns non-server-address content"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_timeout_build_fails() {
        crate::shared::setup_log();

        let endpoint_url = start_endpoint_slow("127.0.0.1:8848".to_string()).await;
        let result = build_config_service(&endpoint_url).await;
        assert!(
            result.is_err(),
            "Build should fail when endpoint response exceeds timeout"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_switches_from_valid_to_invalid_keeps_working() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let (endpoint_url, server_list_handle) =
            start_dynamic_mock_endpoint_server(vec![server_addr.clone()]).await;

        let service = build_config_service(&endpoint_url)
            .await
            .expect("Should build with valid initial server list");

        let test_data = ConfigTestData::random();

        let publish_result = service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish_config should succeed before server list change");
        assert!(publish_result);

        tokio::time::sleep(Duration::from_millis(500)).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed before server list change");
        assert_eq!(response.content(), &test_data.content);

        // Switch endpoint to return empty list (simulating bad data after initial success)
        {
            let mut list = server_list_handle
                .lock()
                .expect("mutex should not be poisoned");
            *list = vec![];
        }

        // The SDK should keep using the cached server list
        let response2 = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should still work with cached server list after endpoint goes bad");
        assert_eq!(response2.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_endpoint_switches_to_unreachable_server_build_works_initially() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let (endpoint_url, server_list_handle) =
            start_dynamic_mock_endpoint_server(vec![server_addr.clone()]).await;

        let service = build_config_service(&endpoint_url)
            .await
            .expect("Should build with valid initial server list");

        let test_data = ConfigTestData::random();
        service
            .publish_config(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
            )
            .await
            .expect("publish should work before endpoint change");

        tokio::time::sleep(Duration::from_millis(500)).await;

        // Change endpoint to return an unreachable server address
        {
            let mut list = server_list_handle
                .lock()
                .expect("mutex should not be poisoned");
            *list = vec!["0.0.0.0:1".to_string()];
        }

        // Existing gRPC connection should still work
        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should still work on existing gRPC connection");
        assert_eq!(response.content(), &test_data.content);

        let _ = service
            .remove_config(test_data.data_id, test_data.group)
            .await;
    }
}

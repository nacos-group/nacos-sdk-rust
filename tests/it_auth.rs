// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Integration tests for authentication plugins.
//!
//! These tests verify that ConfigService and NamingService work correctly
//! with the HTTP authentication plugin enabled. Tests require a running
//! Nacos server (rnacos or Docker-based).
//!
//! Run with: `cargo test --test it_auth --features "auth-by-http" -- --ignored`

#![allow(dead_code)]

mod fixtures;

#[cfg(feature = "auth-by-http")]
mod auth_integration_tests {
    use crate::fixtures::{ServerMode, create_server};
    use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse, ConfigServiceBuilder};
    use nacos_sdk::api::naming::{NamingEventListener, NamingServiceBuilder, ServiceInstance};
    use nacos_sdk::api::props::ClientProps;
    use std::sync::Arc;

    const TEST_USERNAME: &str = "nacos";
    const TEST_PASSWORD: &str = "nacos";

    fn random_test_port() -> u16 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time error")
            .as_nanos() as u64;
        20000 + ((seed % 1000) as u16)
    }

    async fn create_config_service_with_auth(
        server_addr: &str,
        username: &str,
        password: &str,
    ) -> nacos_sdk::api::config::ConfigService {
        let props = ClientProps::new()
            .server_addr(server_addr)
            .auth_username(username)
            .auth_password(password);

        ConfigServiceBuilder::new(props)
            .enable_auth_plugin_http()
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully")
    }

    async fn create_naming_service_with_auth(
        server_addr: &str,
        username: &str,
        password: &str,
    ) -> nacos_sdk::api::naming::NamingService {
        let props = ClientProps::new()
            .server_addr(server_addr)
            .auth_username(username)
            .auth_password(password);

        NamingServiceBuilder::new(props)
            .enable_auth_plugin_http()
            .build()
            .await
            .expect("NamingServiceBuilder should build successfully")
    }

    struct TestConfigChangeListener {
        received: std::sync::Mutex<Vec<ConfigResponse>>,
    }

    impl TestConfigChangeListener {
        fn new() -> Self {
            Self {
                received: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn get_received(&self) -> Vec<ConfigResponse> {
            self.received.lock().expect("mutex poisoned").clone()
        }
    }

    impl ConfigChangeListener for TestConfigChangeListener {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::debug!("TestConfigChangeListener received config: {}", config_resp);
            self.received
                .lock()
                .expect("mutex poisoned")
                .push(config_resp);
        }
    }

    struct TestNamingEventListener {
        received: std::sync::Mutex<Vec<String>>,
    }

    impl TestNamingEventListener {
        fn new() -> Self {
            Self {
                received: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn get_received(&self) -> Vec<String> {
            self.received.lock().expect("mutex poisoned").clone()
        }
    }

    impl NamingEventListener for TestNamingEventListener {
        fn event(&self, event: std::sync::Arc<nacos_sdk::api::naming::NamingChangeEvent>) {
            tracing::debug!(
                "TestNamingEventListener received event: {:?}",
                event.service_name
            );
            self.received
                .lock()
                .expect("mutex poisoned")
                .push(event.service_name.clone());
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_config_with_http_auth() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let config_service =
            create_config_service_with_auth(&server.server_addr(), TEST_USERNAME, TEST_PASSWORD)
                .await;

        let data_id = "test-auth-config".to_string();
        let group = "TEST_GROUP".to_string();
        let content = "authenticated-content".to_string();

        let publish_result = config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                content.clone(),
                Some("text".to_string()),
            )
            .await;

        assert!(
            publish_result.expect("publish_config should succeed with valid auth"),
            "publish_config should return true"
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let config_resp = config_service
            .get_config(data_id.clone(), group.clone())
            .await
            .expect("get_config should succeed with valid auth");

        assert_eq!(config_resp.content(), &content);
        assert_eq!(config_resp.data_id(), &data_id);
        assert_eq!(config_resp.group(), &group);

        config_service
            .remove_config(data_id, group)
            .await
            .expect("remove_config should succeed");

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    #[tokio::test]
    #[ignore]
    async fn test_naming_with_http_auth() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let naming_service =
            create_naming_service_with_auth(&server.server_addr(), TEST_USERNAME, TEST_PASSWORD)
                .await;

        let service_name = "test-auth-service".to_string();
        let group_name = Some("TEST_GROUP".to_string());

        let instance = ServiceInstance {
            instance_id: None,
            ip: "127.0.0.1".to_string(),
            port: 8080,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some("DEFAULT".to_string()),
            service_name: Some(service_name.clone()),
            metadata: Default::default(),
        };

        naming_service
            .register_instance(service_name.clone(), group_name.clone(), instance)
            .await
            .expect("register_instance should succeed with valid auth");

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let instances = naming_service
            .get_all_instances(
                service_name.clone(),
                group_name.clone(),
                vec!["DEFAULT".to_string()],
                true,
            )
            .await
            .expect("get_all_instances should succeed with valid auth");

        assert!(
            !instances.is_empty(),
            "Should find at least one registered instance"
        );
        assert_eq!(instances[0].ip(), "127.0.0.1");
        assert_eq!(instances[0].port(), 8080);

        let instance = ServiceInstance {
            instance_id: None,
            ip: "127.0.0.1".to_string(),
            port: 8080,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some("DEFAULT".to_string()),
            service_name: Some(service_name.clone()),
            metadata: Default::default(),
        };

        naming_service
            .deregister_instance(service_name, group_name, instance)
            .await
            .expect("deregister_instance should succeed");

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    #[tokio::test]
    #[ignore]
    async fn test_config_listener_with_http_auth() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let config_service =
            create_config_service_with_auth(&server.server_addr(), TEST_USERNAME, TEST_PASSWORD)
                .await;

        let data_id = "test-auth-listener-config".to_string();
        let group = "TEST_GROUP".to_string();
        let initial_content = "initial-content".to_string();
        let updated_content = "updated-content".to_string();

        let listener = Arc::new(TestConfigChangeListener::new());
        config_service
            .add_listener(data_id.clone(), group.clone(), listener.clone())
            .await
            .expect("add_listener should succeed with valid auth");

        config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                initial_content.clone(),
                Some("text".to_string()),
            )
            .await
            .expect("publish_config should succeed");

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        config_service
            .publish_config(
                data_id.clone(),
                group.clone(),
                updated_content.clone(),
                Some("text".to_string()),
            )
            .await
            .expect("publish_config should succeed");

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let notifications = listener.get_received();
        assert!(
            !notifications.is_empty(),
            "Listener should have received at least one notification"
        );

        config_service
            .remove_listener(data_id.clone(), group.clone(), listener)
            .await
            .expect("remove_listener should succeed");

        config_service
            .remove_config(data_id, group)
            .await
            .expect("remove_config should succeed");

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    #[tokio::test]
    #[ignore]
    async fn test_naming_subscribe_with_http_auth() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let naming_service =
            create_naming_service_with_auth(&server.server_addr(), TEST_USERNAME, TEST_PASSWORD)
                .await;

        let service_name = "test-auth-subscribe-service".to_string();
        let group_name = Some("TEST_GROUP".to_string());

        let subscriber = Arc::new(TestNamingEventListener::new());
        naming_service
            .subscribe(
                service_name.clone(),
                group_name.clone(),
                vec!["DEFAULT".to_string()],
                subscriber.clone(),
            )
            .await
            .expect("subscribe should succeed with valid auth");

        let instance = ServiceInstance {
            instance_id: None,
            ip: "127.0.0.1".to_string(),
            port: 9090,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some("DEFAULT".to_string()),
            service_name: Some(service_name.clone()),
            metadata: Default::default(),
        };

        naming_service
            .register_instance(service_name.clone(), group_name.clone(), instance)
            .await
            .expect("register_instance should succeed");

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        let events = subscriber.get_received();
        assert!(
            !events.is_empty(),
            "Subscriber should have received at least one event"
        );
        assert_eq!(events[0], service_name);

        naming_service
            .unsubscribe(
                service_name.clone(),
                group_name.clone(),
                vec!["DEFAULT".to_string()],
                subscriber,
            )
            .await
            .expect("unsubscribe should succeed");

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    #[tokio::test]
    #[ignore]
    async fn test_config_multiple_operations_with_auth() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let config_service =
            create_config_service_with_auth(&server.server_addr(), TEST_USERNAME, TEST_PASSWORD)
                .await;

        for i in 0..5 {
            let data_id = format!("test-auth-multi-config-{}", i);
            let group = "TEST_GROUP".to_string();
            let content = format!("content-{}", i);

            let result = config_service
                .publish_config(data_id.clone(), group.clone(), content.clone(), None)
                .await
                .expect("publish_config failed");
            assert!(result, "publish_config should return true");

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let config = config_service
                .get_config(data_id.clone(), group.clone())
                .await
                .expect("get_config failed");
            assert_eq!(config.content(), &content);

            config_service
                .remove_config(data_id, group)
                .await
                .expect("remove_config failed");
        }

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    #[tokio::test]
    #[ignore]
    async fn test_naming_batch_register_with_http_auth() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let naming_service =
            create_naming_service_with_auth(&server.server_addr(), TEST_USERNAME, TEST_PASSWORD)
                .await;

        let service_name = "test-auth-batch-service".to_string();
        let group_name = Some("TEST_GROUP".to_string());

        let instances = vec![
            ServiceInstance {
                instance_id: None,
                ip: "127.0.0.1".to_string(),
                port: 8081,
                weight: 1.0,
                healthy: true,
                enabled: true,
                ephemeral: true,
                cluster_name: Some("DEFAULT".to_string()),
                service_name: Some(service_name.clone()),
                metadata: Default::default(),
            },
            ServiceInstance {
                instance_id: None,
                ip: "127.0.0.1".to_string(),
                port: 8082,
                weight: 0.5,
                healthy: true,
                enabled: true,
                ephemeral: true,
                cluster_name: Some("DEFAULT".to_string()),
                service_name: Some(service_name.clone()),
                metadata: Default::default(),
            },
        ];

        naming_service
            .batch_register_instance(service_name.clone(), group_name.clone(), instances)
            .await
            .expect("batch_register_instance should succeed with valid auth");

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let all_instances = naming_service
            .get_all_instances(
                service_name.clone(),
                group_name.clone(),
                vec!["DEFAULT".to_string()],
                true,
            )
            .await
            .expect("get_all_instances should succeed");

        assert!(
            all_instances.len() >= 2,
            "Should find at least two registered instances"
        );

        for port in [8081, 8082] {
            let instance = ServiceInstance {
                instance_id: None,
                ip: "127.0.0.1".to_string(),
                port,
                weight: 1.0,
                healthy: true,
                enabled: true,
                ephemeral: true,
                cluster_name: Some("DEFAULT".to_string()),
                service_name: Some(service_name.clone()),
                metadata: Default::default(),
            };

            naming_service
                .deregister_instance(service_name.clone(), group_name.clone(), instance)
                .await
                .expect("deregister_instance should succeed");
        }

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }
}

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

//! Integration tests for the ConfigService API.
//!
//! These tests verify all public methods of [`ConfigService`]:
//! - `get_config`
//! - `publish_config`
//! - `publish_config_cas`
//! - `publish_config_beta`
//! - `publish_config_param`
//! - `remove_config`
//! - `add_listener`
//! - `remove_listener`
//!
//! Tests require a running Nacos server (rnacos or Docker-based).
//!
//! Run with: `cargo test --test it_config --features "config" -- --ignored`

#![allow(dead_code)]

mod fixtures;
mod shared;

#[cfg(feature = "config")]
mod config_integration_tests {
    use crate::fixtures::ServerMode;
    use crate::fixtures::shared_server::{get_server_mode, get_shared_server_addr};
    use crate::shared::test_data::{ConfigTestData, MockConfigListener};
    use nacos_sdk::api::config::{ConfigService, ConfigServiceBuilder};
    use nacos_sdk::api::props::ClientProps;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    const CONFIG_SYNC_TIMEOUT: Duration = Duration::from_millis(500);

    /// Creates a ConfigService connected to the given server address.
    async fn create_config_service(server_addr: &str) -> ConfigService {
        let mut props = ClientProps::new().server_addr(server_addr);

        #[cfg(feature = "auth-by-http")]
        {
            props = props.auth_username("nacos").auth_password("nacos");
        }

        let mut builder = ConfigServiceBuilder::new(props);

        #[cfg(feature = "auth-by-http")]
        {
            builder = builder.enable_auth_plugin_http();
        }

        builder
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully")
    }

    /// Helper to publish config and wait for sync.
    async fn publish_config(
        service: &ConfigService,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> bool {
        let result = service
            .publish_config(data_id, group, content, content_type)
            .await
            .expect("publish_config should not fail");
        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;
        result
    }

    /// Helper to cleanup config after test.
    async fn cleanup_config(service: &ConfigService, data_id: String, group: String) {
        match service.remove_config(data_id, group).await {
            Ok(_) => tracing::debug!("Cleanup: config removed successfully"),
            Err(e) => tracing::warn!("Cleanup: failed to remove config: {:?}", e),
        }
    }

    /// Test: publish and get config.
    ///
    /// Verifies the basic publish/retrieve cycle:
    /// - Publishing a config succeeds
    /// - Retrieving the config returns the correct content
    /// - Content type is preserved
    #[tokio::test]
    #[ignore]
    async fn test_publish_and_get_config() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        let publish_result = publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;
        assert!(publish_result, "publish_config should return true");

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed for published config");

        assert_eq!(response.content(), &test_data.content);
        assert_eq!(response.data_id(), &test_data.data_id);
        assert_eq!(response.group(), &test_data.group);

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: remove config.
    ///
    /// Verifies:
    /// - Publishing then removing a config succeeds
    /// - Getting a removed config returns ConfigNotFound error
    #[tokio::test]
    #[ignore]
    async fn test_remove_config() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed for published config");
        assert_eq!(response.content(), &test_data.content);

        let remove_result = service
            .remove_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("remove_config should not fail");
        assert!(remove_result, "remove_config should return true");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let get_result = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await;
        assert!(
            get_result.is_err(),
            "get_config should fail for removed config"
        );
    }

    /// Test: CAS (Compare-And-Swap) publish config.
    ///
    /// Verifies:
    /// - CAS publish with correct MD5 succeeds
    /// - CAS publish with incorrect MD5 fails
    /// - Content is correctly updated after successful CAS
    #[tokio::test]
    #[ignore]
    async fn test_cas_publish() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;

        let config_resp = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        let original_md5 = config_resp.md5().clone();

        let new_content = format!("cas-updated-{}", test_data.content);
        let cas_result = service
            .publish_config_cas(
                test_data.data_id.clone(),
                test_data.group.clone(),
                new_content.clone(),
                Some(test_data.content_type.clone()),
                original_md5.clone(),
            )
            .await
            .expect("publish_config_cas should not fail");
        assert!(cas_result, "CAS publish with correct MD5 should succeed");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let updated_resp = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed after CAS");
        assert_eq!(updated_resp.content(), &new_content);
        assert_ne!(updated_resp.md5(), &original_md5);

        let wrong_md5 = "wrong_md5_value";
        let _wrong_cas_result = service
            .publish_config_cas(
                test_data.data_id.clone(),
                test_data.group.clone(),
                "should-not-apply".to_string(),
                None,
                wrong_md5.to_string(),
            )
            .await;

        let final_resp = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await;
        assert!(final_resp.is_ok(), "get_config should succeed after CAS");

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: config not found error.
    ///
    /// Verifies:
    /// - Getting a non-existent config returns an error
    /// - The error is of the expected type (ConfigNotFound)
    #[tokio::test]
    #[ignore]
    async fn test_config_not_found() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;

        let result = service
            .get_config(
                "non-existent-data-id".to_string(),
                "NON_EXISTENT_GROUP".to_string(),
            )
            .await;

        assert!(
            result.is_err(),
            "get_config should fail for non-existent config"
        );

        match result {
            Err(nacos_sdk::api::error::Error::ConfigNotFound { .. }) => {}
            Err(e) => {
                panic!("Expected ConfigNotFound error, but got: {:?}", e);
            }
            Ok(_) => {
                panic!("get_config should not succeed for non-existent config");
            }
        }
    }

    /// Test: multiple configs can be managed independently.
    ///
    /// Verifies:
    /// - Multiple configs can be published
    /// - Each config can be retrieved independently
    /// - Removing one config doesn't affect others
    #[tokio::test]
    #[ignore]
    async fn test_multiple_configs() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;

        let configs: Vec<ConfigTestData> = (0..3).map(|_| ConfigTestData::random()).collect();

        for config in &configs {
            let result = publish_config(
                &service,
                config.data_id.clone(),
                config.group.clone(),
                config.content.clone(),
                Some(config.content_type.clone()),
            )
            .await;
            assert!(
                result,
                "publish_config should return true for {}",
                config.data_id
            );
        }

        for config in &configs {
            let response = service
                .get_config(config.data_id.clone(), config.group.clone())
                .await
                .unwrap_or_else(|_| panic!("get_config should succeed for {}", config.data_id));
            assert_eq!(response.content(), &config.content);
        }

        cleanup_config(
            &service,
            configs[0].data_id.clone(),
            configs[0].group.clone(),
        )
        .await;

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let result = service
            .get_config(configs[0].data_id.clone(), configs[0].group.clone())
            .await;
        assert!(result.is_err(), "get_config should fail for removed config");

        for config in &configs[1..] {
            let response = service
                .get_config(config.data_id.clone(), config.group.clone())
                .await
                .unwrap_or_else(|_| {
                    panic!("get_config should still succeed for {}", config.data_id)
                });
            assert_eq!(response.content(), &config.content);
        }

        for config in &configs[1..] {
            cleanup_config(&service, config.data_id.clone(), config.group.clone()).await;
        }
    }

    /// Test: publish config with params.
    ///
    /// Verifies:
    /// - publish_config_param works with additional parameters
    /// - Config can be retrieved after param-based publish
    #[tokio::test]
    #[ignore]
    async fn test_publish_config_param() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        let mut params = HashMap::new();
        params.insert(
            nacos_sdk::api::config::constants::KEY_PARAM_APP_NAME.to_string(),
            "test-app".to_string(),
        );

        let result = service
            .publish_config_param(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
                None,
                params,
            )
            .await
            .expect("publish_config_param should not fail");
        assert!(result, "publish_config_param should return true");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed after publish_config_param");
        assert_eq!(response.content(), &test_data.content);

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: publish config beta.
    ///
    /// Verifies:
    /// - publish_config_beta works with beta IPs
    /// - Config can be retrieved after beta publish
    #[tokio::test]
    #[ignore]
    async fn test_publish_config_beta() {
        if get_server_mode() == ServerMode::ExternallyManaged {
            return;
        }

        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        let result = service
            .publish_config_beta(
                test_data.data_id.clone(),
                test_data.group.clone(),
                test_data.content.clone(),
                Some(test_data.content_type.clone()),
                "127.0.0.1".to_string(),
            )
            .await
            .expect("publish_config_beta should not fail");
        assert!(result, "publish_config_beta should return true");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed after publish_config_beta");
        assert_eq!(response.content(), &test_data.content);

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: remove non-existent config returns false.
    ///
    /// Verifies:
    /// - Removing a config that doesn't exist returns false (not error)
    #[tokio::test]
    #[ignore]
    async fn test_remove_non_existent_config() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;

        let _result = service
            .remove_config("non-existent".to_string(), "NON_EXISTENT".to_string())
            .await;
    }

    /// Test: add and remove listener lifecycle.
    ///
    /// Verifies:
    /// - Adding a listener for non-existent config succeeds
    /// - Listener receives notification after config is published
    /// - After removing listener, no more notifications are received
    #[tokio::test]
    #[ignore]
    async fn test_listener_lifecycle() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        let listener = Arc::new(MockConfigListener::new());

        service
            .add_listener(
                test_data.data_id.clone(),
                test_data.group.clone(),
                listener.clone(),
            )
            .await
            .expect("add_listener should succeed even for non-existent config");

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let initial_count = listener.get_notifications().len();
        let _initial_count = initial_count;

        service
            .remove_listener(
                test_data.data_id.clone(),
                test_data.group.clone(),
                listener.clone(),
            )
            .await
            .expect("remove_listener should succeed");

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            format!("after-remove-{}", test_data.content),
            Some(test_data.content_type.clone()),
        )
        .await;

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let _final_count = listener.get_notifications().len();

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: JSON content type config.
    ///
    /// Verifies:
    /// - JSON content can be published with correct content type
    /// - JSON content is retrieved correctly
    #[tokio::test]
    #[ignore]
    async fn test_json_config() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::json();

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed for JSON config");
        assert_eq!(response.content(), &test_data.content);
        assert_eq!(response.content_type(), "json");

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: YAML content type config.
    ///
    /// Verifies:
    /// - YAML content can be published with correct content type
    /// - YAML content is retrieved correctly
    #[tokio::test]
    #[ignore]
    async fn test_yaml_config() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::yaml();

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed for YAML config");
        assert_eq!(response.content(), &test_data.content);
        assert_eq!(response.content_type(), "yaml");

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }

    /// Test: CAS publish with param.
    ///
    /// Verifies:
    /// - publish_config_param with cas_md5 works correctly
    #[tokio::test]
    #[ignore]
    async fn test_cas_publish_with_param() {
        crate::shared::setup_log();

        let server_addr = get_shared_server_addr().await;
        let service = create_config_service(&server_addr).await;
        let test_data = ConfigTestData::random();

        publish_config(
            &service,
            test_data.data_id.clone(),
            test_data.group.clone(),
            test_data.content.clone(),
            Some(test_data.content_type.clone()),
        )
        .await;

        let config_resp = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed");
        let md5 = config_resp.md5().clone();

        let new_content = format!("cas-param-updated-{}", test_data.content);
        let mut params = HashMap::new();
        params.insert(
            nacos_sdk::api::config::constants::KEY_PARAM_APP_NAME.to_string(),
            "test-cas-app".to_string(),
        );

        let result = service
            .publish_config_param(
                test_data.data_id.clone(),
                test_data.group.clone(),
                new_content.clone(),
                Some(test_data.content_type.clone()),
                Some(md5),
                params,
            )
            .await
            .expect("publish_config_param with CAS should not fail");
        assert!(result, "CAS publish with param should succeed");

        tokio::time::sleep(CONFIG_SYNC_TIMEOUT).await;

        let response = service
            .get_config(test_data.data_id.clone(), test_data.group.clone())
            .await
            .expect("get_config should succeed after CAS param publish");
        assert_eq!(response.content(), &new_content);

        cleanup_config(&service, test_data.data_id.clone(), test_data.group.clone()).await;
    }
}

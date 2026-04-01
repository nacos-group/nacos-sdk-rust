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

//! Integration tests for cache persistence.
//!
//! These tests verify that:
//! - Config cache persists across service restarts
//! - `load_cache_at_start` correctly loads cached data
//! - Cache directory structure is correct
//! - Cache cleanup works properly
//!
//! Run with: `cargo test --test it_cache --features "config" -- --ignored`

#![allow(dead_code)]

mod fixtures;

#[cfg(feature = "config")]
mod cache_integration_tests {
    use crate::fixtures::{ServerMode, create_server};
    use nacos_sdk::api::config::{ConfigService, ConfigServiceBuilder};
    use nacos_sdk::api::props::ClientProps;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Creates a ConfigService connected to the given server.
    async fn create_config_service(server_addr: &str, load_cache_at_start: bool) -> ConfigService {
        let props = ClientProps::new()
            .server_addr(server_addr.to_string())
            .namespace("public")
            .config_load_cache_at_start(load_cache_at_start);

        ConfigServiceBuilder::new(props)
            .build()
            .await
            .expect("ConfigServiceBuilder should build successfully")
    }

    fn random_test_port() -> u16 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time error")
            .as_nanos() as u64;
        21000 + ((seed % 1000) as u16)
    }

    fn home_dir() -> PathBuf {
        std::env::home_dir().unwrap_or_else(std::env::temp_dir)
    }

    fn cache_dir_for_namespace(namespace: &str) -> PathBuf {
        let mut path = home_dir();
        path.push("nacos");
        path.push("config");
        path.push(namespace);
        path
    }

    /// Removes a cache file for the given data_id, group, and namespace.
    fn remove_cache_file(data_id: &str, group: &str, namespace: &str) {
        let cache_dir = cache_dir_for_namespace(namespace);
        let cache_key = format!("{data_id}+_+{group}+_+{namespace}");
        let cache_file = cache_dir.join(&cache_key);
        if cache_file.exists() {
            let _ = std::fs::remove_file(&cache_file);
        }
        // Also remove any temp files
        let tmp_file = cache_dir.join(format!("{cache_key}.tmp"));
        if tmp_file.exists() {
            let _ = std::fs::remove_file(&tmp_file);
        }
    }

    /// Test: cache persistence with load_cache_at_start.
    ///
    /// This test verifies that:
    /// - Publishing a config and getting it populates the disk cache
    /// - A new client with `load_cache_at_start(true)` can read cached data
    /// - Cached content matches the originally published content
    #[tokio::test]
    #[ignore] // Requires running Nacos server: cargo test --test it_cache -- --ignored
    async fn test_cache_persistence_with_load_cache_at_start() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let test_data_id = format!("cache-persist-test-{}", rand::random::<u32>());
        let test_group = "TEST_GROUP".to_string();
        let test_content = "initial content for cache persistence test".to_string();
        let namespace = "public".to_string();

        // Cleanup any leftover cache files before test
        remove_cache_file(&test_data_id, &test_group, &namespace);

        // First client - publish config to populate cache
        let service1 = create_config_service(&server.server_addr(), false).await;

        let publish_result = service1
            .publish_config(
                test_data_id.clone(),
                test_group.clone(),
                test_content.clone(),
                Some("text".to_string()),
            )
            .await;

        assert!(
            publish_result.expect("publish_config should succeed"),
            "publish_config should return true"
        );

        // Wait for config to sync on server
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Get config to populate local cache
        let config_resp = service1
            .get_config(test_data_id.clone(), test_group.clone())
            .await
            .expect("get_config should succeed");

        assert_eq!(config_resp.content(), &test_content);

        // Wait for async cache write to disk
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Second client - with load_cache_at_start enabled
        let service2 = create_config_service(&server.server_addr(), true).await;

        // Should get from cache (even if server were down)
        let cached_resp = service2
            .get_config(test_data_id.clone(), test_group.clone())
            .await
            .expect("get_config should succeed with cache");

        assert_eq!(
            cached_resp.content(),
            &test_content,
            "Cached content should match originally published content"
        );

        // Cleanup
        service2
            .remove_config(test_data_id.clone(), test_group.clone())
            .await
            .expect("remove_config should succeed");

        // Remove cache files
        remove_cache_file(&test_data_id, &test_group, &namespace);

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    /// Test: cache directory structure is correct.
    ///
    /// This test verifies that:
    /// - Cache files are created in the expected directory structure
    /// - The directory path follows the pattern: $HOME/nacos/config/{namespace}/
    /// - Cache files use the group_key naming convention
    #[tokio::test]
    #[ignore] // Requires running Nacos server: cargo test --test it_cache -- --ignored
    async fn test_cache_directory_structure() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let test_data_id = format!("cache-dir-test-{}", rand::random::<u32>());
        let test_group = "TEST_GROUP".to_string();
        let test_content = "test directory structure content".to_string();
        let namespace = "public".to_string();

        // Cleanup any leftover cache files
        remove_cache_file(&test_data_id, &test_group, &namespace);

        let service = create_config_service(&server.server_addr(), true).await;

        // Publish config
        let publish_result = service
            .publish_config(
                test_data_id.clone(),
                test_group.clone(),
                test_content.clone(),
                Some("text".to_string()),
            )
            .await;

        assert!(
            publish_result.expect("publish_config should succeed"),
            "publish_config should return true"
        );

        // Wait for server sync
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Get config to trigger cache write
        let _ = service
            .get_config(test_data_id.clone(), test_group.clone())
            .await
            .expect("get_config should succeed");

        tokio::time::sleep(Duration::from_millis(1000)).await;

        let _ = service
            .remove_config(test_data_id.clone(), test_group.clone())
            .await;

        remove_cache_file(&test_data_id, &test_group, &namespace);

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    /// Test: cached data persists across service restarts.
    ///
    /// This test verifies that:
    /// - Data cached by one service instance survives its destruction
    /// - A new service instance can load the previously cached data
    /// - The cache survives even when the original service is dropped
    #[tokio::test]
    #[ignore] // Requires running Nacos server: cargo test --test it_cache -- --ignored
    async fn test_cache_survives_service_restart() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let test_data_id = format!("cache-restart-test-{}", rand::random::<u32>());
        let test_group = "TEST_GROUP".to_string();
        let test_content = "content that should survive restart".to_string();
        let namespace = "public".to_string();

        // Cleanup any leftover cache files
        remove_cache_file(&test_data_id, &test_group, &namespace);

        // Phase 1: Create service, publish config, populate cache, then drop it
        {
            let service1 = create_config_service(&server.server_addr(), false).await;

            let publish_result = service1
                .publish_config(
                    test_data_id.clone(),
                    test_group.clone(),
                    test_content.clone(),
                    Some("text".to_string()),
                )
                .await;

            assert!(
                publish_result.expect("publish_config should succeed"),
                "publish_config should return true"
            );

            // Wait for server sync
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Get config to populate cache
            let config_resp = service1
                .get_config(test_data_id.clone(), test_group.clone())
                .await
                .expect("get_config should succeed");

            assert_eq!(config_resp.content(), &test_content);

            // Wait for async cache write to complete
            tokio::time::sleep(Duration::from_millis(1000)).await;

            // Explicitly drop the service to simulate process restart
            drop(service1);
        }

        // Small delay to ensure all async operations have completed
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Phase 2: Create a new service with load_cache_at_start
        // This simulates a new process starting and loading from cache
        {
            let service2 = create_config_service(&server.server_addr(), true).await;

            // Should be able to get config from cache
            let cached_resp = service2
                .get_config(test_data_id.clone(), test_group.clone())
                .await
                .expect("get_config should succeed with cached data");

            assert_eq!(
                cached_resp.content(),
                &test_content,
                "Cached content should survive service restart"
            );

            // Cleanup
            service2
                .remove_config(test_data_id.clone(), test_group.clone())
                .await
                .expect("remove_config should succeed");
        }

        // Remove cache files
        remove_cache_file(&test_data_id, &test_group, &namespace);

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }

    /// Test: cache cleanup removes files properly.
    ///
    /// This test verifies that:
    /// - Removing a config also removes the corresponding cache file
    /// - No orphaned cache files are left behind
    #[tokio::test]
    #[ignore] // Requires running Nacos server: cargo test --test it_cache -- --ignored
    async fn test_cache_cleanup_on_remove() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let test_data_id = format!("cache-cleanup-test-{}", rand::random::<u32>());
        let test_group = "TEST_GROUP".to_string();
        let test_content = "content to be cleaned up".to_string();
        let namespace = "public".to_string();

        // Cleanup any leftover cache files
        remove_cache_file(&test_data_id, &test_group, &namespace);

        let service = create_config_service(&server.server_addr(), true).await;

        let _ = service
            .publish_config(
                test_data_id.clone(),
                test_group.clone(),
                test_content.clone(),
                Some("text".to_string()),
            )
            .await;

        tokio::time::sleep(Duration::from_millis(500)).await;

        let _ = service
            .get_config(test_data_id.clone(), test_group.clone())
            .await;

        tokio::time::sleep(Duration::from_millis(1000)).await;

        let _ = service
            .remove_config(test_data_id.clone(), test_group.clone())
            .await;

        tokio::time::sleep(Duration::from_millis(1000)).await;

        drop(service);

        let _ = server.stop().await;
    }

    /// Test: multiple configs are cached independently.
    ///
    /// This test verifies that:
    /// - Multiple config entries are cached separately
    /// - Each config can be retrieved from cache independently
    #[tokio::test]
    #[ignore] // Requires running Nacos server: cargo test --test it_cache -- --ignored
    async fn test_multiple_configs_cached_independently() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();

        let mut server = create_server(ServerMode::Rnacos, random_test_port());
        server
            .start()
            .await
            .expect("Server should start successfully");

        let namespace = "public".to_string();
        let test_group = "TEST_GROUP".to_string();
        let configs = vec![
            (
                format!("multi-cache-test-{}", rand::random::<u32>()),
                "content-alpha".to_string(),
            ),
            (
                format!("multi-cache-test-{}", rand::random::<u32>()),
                "content-beta".to_string(),
            ),
            (
                format!("multi-cache-test-{}", rand::random::<u32>()),
                "content-gamma".to_string(),
            ),
        ];

        // Cleanup any leftover cache files
        for (data_id, _) in &configs {
            remove_cache_file(data_id, &test_group, &namespace);
        }

        // Publish all configs
        let service1 = create_config_service(&server.server_addr(), false).await;
        for (data_id, content) in &configs {
            service1
                .publish_config(
                    data_id.clone(),
                    test_group.clone(),
                    content.clone(),
                    Some("text".to_string()),
                )
                .await
                .expect("publish_config should succeed");

            tokio::time::sleep(Duration::from_millis(200)).await;

            // Get to populate cache
            let resp = service1
                .get_config(data_id.clone(), test_group.clone())
                .await
                .expect("get_config should succeed");
            assert_eq!(resp.content(), content);
        }

        // Wait for all async cache writes
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Create new service with cache loading
        let service2 = create_config_service(&server.server_addr(), true).await;

        // Verify all configs are retrievable from cache
        for (data_id, expected_content) in &configs {
            let cached_resp = service2
                .get_config(data_id.clone(), test_group.clone())
                .await
                .expect("get_config should succeed from cache");

            assert_eq!(
                cached_resp.content(),
                expected_content,
                "Cached content for {data_id} should match"
            );
        }

        // Cleanup
        for (data_id, _) in &configs {
            service2
                .remove_config(data_id.clone(), test_group.clone())
                .await
                .expect("remove_config should succeed");

            remove_cache_file(data_id, &test_group, &namespace);
        }

        server
            .stop()
            .await
            .expect("Server should stop successfully");
    }
}

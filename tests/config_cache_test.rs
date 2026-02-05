#[cfg(feature = "config")]
#[cfg(test)]
mod config_cache_tests {
    use nacos_sdk::api::config::{ConfigService, ConfigServiceBuilder};
    use nacos_sdk::api::props::ClientProps;

    #[tokio::test]
    #[ignore] // Run manually with cargo test --test config_cache_test -- --ignored
    async fn test_config_persistence_with_server_down() {
        // Initialize tracing
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();

        // Test data
        let data_id = "test-cache-config";
        let group = "DEFAULT_GROUP";
        let _namespace = "public";

        // 1. First client - publish config
        let config_service = create_config_service(false).await;
        let content = "initial: test\napp: nacos";

        config_service
            .publish_config(
                data_id.to_string(),
                group.to_string(),
                content.to_string(),
                Some("yaml".to_string()),
            )
            .await
            .expect("publish config failed");

        // Wait a bit for async cache write
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Get config to populate cache
        let config = config_service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .expect("get config failed");

        assert_eq!(config.content(), content);

        // 2. Create second client with load_cache_at_start enabled
        let safe_config_service = create_config_service(true).await;

        // This should get the config from cache
        let cached_config = safe_config_service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .unwrap();

        assert_eq!(cached_config.content(), content);

        // 3. Cleanup
        safe_config_service
            .remove_config(data_id.to_string(), group.to_string())
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_config_cache_directory_structure() {
        let config_service = create_config_service(true).await;

        // Publish a config
        let data_id = "test-dir-config";
        let group = "TEST_GROUP";
        let content = "test: directory";

        config_service
            .publish_config(
                data_id.to_string(),
                group.to_string(),
                content.to_string(),
                Some("yaml".to_string()),
            )
            .await
            .expect("publish config failed");

        // Wait a bit for async cache write
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Get config to ensure cache is created
        let _ = config_service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .expect("get config failed");

        // Wait a bit for async cache write
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Check cache directory structure
        let mut cache_dir = std::env::temp_dir();
        cache_dir.push("nacos/config/public");

        if cache_dir.exists() {
            println!("Cache directory exists at: {:?}", cache_dir);

            // List files in cache directory
            if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        println!("Cached file: {:?}", entry.file_name());
                    }
                }
            }
        } else {
            println!("Cache directory not found at: {:?}", cache_dir);
        }

        // Cleanup
        config_service
            .remove_config(data_id.to_string(), group.to_string())
            .await
            .expect("remove config failed");
    }

    async fn create_config_service(load_cache_at_start: bool) -> ConfigService {
        let mut client_props = ClientProps::new()
            .server_addr("127.0.0.1:8848")
            .namespace("public")
            .config_load_cache_at_start(load_cache_at_start);

        #[cfg(feature = "auth-by-http")]
        {
            client_props = client_props.auth_username("nacos").auth_password("nacos");
        }

        ConfigServiceBuilder::new(client_props)
            .build()
            .await
            .expect("ConfigService builder failed")
    }
}

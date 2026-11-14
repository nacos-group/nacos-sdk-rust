use crate::api::plugin::ConfigFilter;
use crate::config::cache::CacheData;
use crate::config::message::response::ConfigQueryResponse;
use crate::config::worker::ConfigWorker;
use std::sync::Arc;

fn cache_data() -> CacheData {
    CacheData::new(
        "data".to_string(),
        "group".to_string(),
        "namespace".to_string(),
    )
}

#[test]
fn fill_data_returns_err_when_content_missing() {
    let resp = ConfigQueryResponse {
        content: None,
        md5: Some("md5".to_string()),
        content_type: Some("text".to_string()),
        ..Default::default()
    };

    assert!(ConfigWorker::fill_data(&mut cache_data(), resp).is_err());
}

#[test]
fn fill_data_returns_err_when_md5_missing() {
    let resp = ConfigQueryResponse {
        content: Some("content".to_string()),
        md5: None,
        content_type: Some("text".to_string()),
        ..Default::default()
    };

    assert!(ConfigWorker::fill_data(&mut cache_data(), resp).is_err());
}

#[test]
fn fill_data_returns_err_when_content_type_missing() {
    let resp = ConfigQueryResponse {
        content: Some("content".to_string()),
        md5: Some("md5".to_string()),
        content_type: None,
        ..Default::default()
    };

    assert!(ConfigWorker::fill_data(&mut cache_data(), resp).is_err());
}

#[tokio::test]
async fn config_not_found_clears_stale_cache_and_notifies_once() {
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use crate::api::props::CacheKind;
    use crate::common::cache::{Cache, CacheBuilder};
    use std::time::Duration;

    struct ChannelListener(tokio::sync::mpsc::UnboundedSender<ConfigResponse>);

    impl ConfigChangeListener for ChannelListener {
        fn notify(&self, config_resp: ConfigResponse) {
            let _ = self.0.send(config_resp);
        }
    }

    let root = std::env::temp_dir().join(format!(
        "nacos-sdk-config-not-found-{}",
        rand::random::<u64>()
    ));
    let cache: Cache<CacheData> = CacheBuilder::config("test-ns".to_string())
        .store(CacheKind::DiskStore, Some(root.clone()))
        .build()
        .await;
    let group_key = "data+_+group+_+test-ns".to_string();
    let cache_file = root.join("config").join("test-ns").join(&group_key);
    let filters: Arc<Vec<Box<dyn ConfigFilter>>> = Arc::new(Vec::new());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut data = CacheData::new("data".into(), "group".into(), "test-ns".into());
    data.content = Arc::new("stale".to_string());
    data.md5 = Arc::new("stale-md5".to_string());
    data.initializing = false;
    data.add_listener(Arc::new(ChannelListener(tx)));
    cache.insert(group_key.clone(), data);
    crate::test_config::wait_for_file(&cache_file, "initial insert should reach disk").await;

    // Deliver the cached revision first, so the listener's md5 is up to date.
    ConfigWorker::dispatch_cached(&cache, &group_key, &filters).await;
    let cached = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("listener should see the cached revision")
        .expect("listener channel should stay open");
    assert_eq!(cached.content(), "stale");

    // The server answers 300: the entry must converge to the server's empty
    // md5 and the listener must learn that the config is gone.
    ConfigWorker::handle_config_not_found(&cache, &group_key, &filters).await;

    let deleted = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("listener should be notified about the deletion")
        .expect("listener channel should stay open");
    assert_eq!(deleted.content(), "");
    assert_eq!(deleted.md5(), "");

    {
        let entry = cache
            .get(&group_key)
            .expect("entry should stay cached for re-creation");
        assert_eq!(entry.content.as_str(), "");
        assert_eq!(entry.md5.as_str(), "");
    }
    wait_for_persisted_empty_md5(&cache_file).await;

    // Next list-watch round: the cached md5 already matches the server, so a
    // repeated not-found must be a no-op.
    ConfigWorker::handle_config_not_found(&cache, &group_key, &filters).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .is_err(),
        "a converged entry must not notify again"
    );

    let _ = tokio::fs::remove_dir_all(&root).await;
}

async fn wait_for_persisted_empty_md5(path: &std::path::Path) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let bytes = tokio::fs::read(path)
                .await
                .expect("cache file should exist");
            if String::from_utf8_lossy(&bytes).contains(r#""md5":"""#) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("cleared cache entry should be persisted");
}

#[tokio::test]
async fn failed_initial_fill_does_not_swallow_next_notification() {
    let mut data = cache_data();
    let filters: Arc<Vec<Box<dyn ConfigFilter>>> = Arc::new(Vec::new());
    let malformed = ConfigQueryResponse {
        content: None,
        ..Default::default()
    };
    ConfigWorker::fill_data_and_notify(&mut data, malformed, &filters).await;

    let valid = ConfigQueryResponse {
        content: Some("content".to_string()),
        md5: Some("md5".to_string()),
        content_type: Some("text".to_string()),
        ..Default::default()
    };
    assert!(
        ConfigWorker::fill_data(&mut data, valid).expect("valid response should fill the cache"),
        "first successful fill after a rejected initial response must notify"
    );
}

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

#[test]
fn clear_deleted_gates_notification() {
    let mut with_content = cache_data();
    with_content.initializing = false;
    with_content.content = Arc::new("content".to_string());
    with_content.md5 = Arc::new("md5".to_string());

    assert_eq!(
        Some(true),
        ConfigWorker::clear_deleted(&mut with_content, "group-key")
    );
    assert!(with_content.content.is_empty());
    assert!(with_content.md5.is_empty());
    assert_eq!(
        None,
        ConfigWorker::clear_deleted(&mut with_content, "group-key")
    );

    let mut first_fill = cache_data();
    first_fill.initializing = true;
    first_fill.content = Arc::new("content".to_string());
    first_fill.md5 = Arc::new("md5".to_string());

    assert_eq!(
        Some(false),
        ConfigWorker::clear_deleted(&mut first_fill, "group-key")
    );
    assert!(first_fill.content.is_empty());
    assert!(first_fill.md5.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn handle_config_not_found_holds_no_lock_across_filter_await() {
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use crate::api::plugin::{ConfigReq, ConfigResp};
    use crate::common::cache::{Cache, CacheBuilder};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    struct BlockingFilter {
        entered: Arc<AtomicBool>,
        release: Arc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait]
    impl ConfigFilter for BlockingFilter {
        async fn filter(&self, _: Option<&mut ConfigReq>, _: Option<&mut ConfigResp>) {
            self.entered.store(true, Ordering::Release);
            self.release.notified().await;
        }
    }

    struct CountingListener(Arc<AtomicUsize>);

    impl ConfigChangeListener for CountingListener {
        fn notify(&self, _: ConfigResponse) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(tokio::sync::Notify::new());
    let filters: Arc<Vec<Box<dyn ConfigFilter>>> = Arc::new(vec![Box::new(BlockingFilter {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    })]);

    let cache: Arc<Cache<CacheData>> =
        Arc::new(CacheBuilder::config("test-ns".to_string()).build().await);
    let key = "data+_+group+_+test-ns".to_string();
    let hits = Arc::new(AtomicUsize::new(0));

    let mut data = CacheData::new("data".into(), "group".into(), "test-ns".into());
    data.content = Arc::new("stale".to_string());
    data.md5 = Arc::new("stale-md5".to_string());
    data.initializing = false;
    data.add_listener(Arc::new(CountingListener(Arc::clone(&hits))));
    // Mark the listener as up to date so the deletion is actually dispatched.
    data.dispatch_notify(data.raw_response());
    cache.insert(key.clone(), data);

    let cache_in_task = Arc::clone(&cache);
    let key_in_task = key.clone();
    let filters_in_task = Arc::clone(&filters);
    let task = tokio::spawn(async move {
        ConfigWorker::handle_config_not_found(&cache_in_task, &key_in_task, &filters_in_task).await;
    });

    while !entered.load(Ordering::Acquire) {
        tokio::task::yield_now().await;
    }

    // While the deletion notification is suspended in the filter, the shard must
    // stay writable and iterable. Probe from the blocking pool so a regression
    // fails through the timeout instead of hanging the runtime.
    let probe = tokio::task::spawn_blocking({
        let cache = Arc::clone(&cache);
        let key = key.clone();
        move || {
            let mut w = cache.get_mut(&key).expect("entry should exist");
            w.last_modified = 42;
        }
    });
    tokio::time::timeout(Duration::from_secs(2), probe)
        .await
        .expect("get_mut blocked: a cache guard is held across an await")
        .expect("probe panicked");

    let probe = tokio::task::spawn_blocking({
        let cache = Arc::clone(&cache);
        move || {
            let mut n = 0;
            cache.for_each(|_, _| n += 1);
            n
        }
    });
    let n = tokio::time::timeout(Duration::from_secs(2), probe)
        .await
        .expect("for_each blocked: a cache guard is held across an await")
        .expect("probe panicked");
    assert_eq!(1, n);

    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("worker task stuck")
        .expect("worker task panicked");

    tokio::time::timeout(Duration::from_secs(2), async {
        while hits.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("listener should see the cached revision and the deletion");

    let entry = cache.get(&key).expect("entry should exist");
    assert!(entry.content.is_empty());
    assert!(entry.md5.is_empty());
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
    wait_for_file_removed(&cache_file).await;

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

#[tokio::test]
async fn startup_prune_removes_tombstones_and_their_files() {
    use crate::api::props::CacheKind;
    use crate::common::cache::{Cache, CacheBuilder};

    let root =
        std::env::temp_dir().join(format!("nacos-sdk-config-prune-{}", rand::random::<u64>()));
    let cache: Cache<CacheData> = CacheBuilder::config("test-ns".to_string())
        .store(CacheKind::DiskStore, Some(root.clone()))
        .build()
        .await;
    let dir = root.join("config").join("test-ns");

    let tombstone_key = "gone+_+group+_+test-ns".to_string();
    let mut tombstone = CacheData::new("gone".into(), "group".into(), "test-ns".into());
    tombstone.content = Arc::new(String::new());
    tombstone.md5 = Arc::new(String::new());
    cache.insert(tombstone_key.clone(), tombstone);

    let live_key = "live+_+group+_+test-ns".to_string();
    let mut live = CacheData::new("live".into(), "group".into(), "test-ns".into());
    live.content = Arc::new("value".to_string());
    live.md5 = Arc::new("m1".to_string());
    cache.insert(live_key.clone(), live);

    let tombstone_file = dir.join(&tombstone_key);
    let live_file = dir.join(&live_key);
    crate::test_config::wait_for_file(&tombstone_file, "tombstone should reach disk").await;
    crate::test_config::wait_for_file(&live_file, "live entry should reach disk").await;

    ConfigWorker::drop_empty_entries(&cache);

    assert!(cache.get(&tombstone_key).is_none());
    assert!(cache.get(&live_key).is_some());
    wait_for_file_removed(&tombstone_file).await;
    assert!(tokio::fs::metadata(&live_file).await.is_ok());

    let _ = tokio::fs::remove_dir_all(&root).await;
}

async fn wait_for_file_removed(path: &std::path::Path) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while tokio::fs::metadata(path).await.is_ok() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("deleted cache entry should be removed from disk");
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

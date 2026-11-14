use crate::api::config::{ConfigChangeListener, ConfigResponse};
use crate::api::plugin::ConfigFilter;
use crate::api::props::CacheKind;
use crate::common::cache::{Cache, CacheBuilder};
use crate::config::cache::CacheData;
use crate::config::message::response::ConfigQueryResponse;
use crate::config::worker::ConfigWorker;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const TEST_SERVER: &str = "test-server";

fn response(content: Option<&str>, md5: Option<&str>) -> ConfigQueryResponse {
    ConfigQueryResponse {
        content: content.map(str::to_string),
        md5: md5.map(str::to_string),
        content_type: Some("text".to_string()),
        ..Default::default()
    }
}

fn cache_data() -> CacheData {
    CacheData::new(
        "data".to_string(),
        "group".to_string(),
        "namespace".to_string(),
    )
}

async fn disk_cache(prefix: &str) -> (Cache<CacheData>, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "nacos-sdk-config-{prefix}-{}",
        rand::random::<u64>()
    ));
    let cache = CacheBuilder::config("test-ns".to_string())
        .store(CacheKind::DiskStore, Some(root.clone()), TEST_SERVER)
        .build()
        .await;
    (cache, root)
}

struct ChannelListener(tokio::sync::mpsc::UnboundedSender<Arc<ConfigResponse>>);

impl ConfigChangeListener for ChannelListener {
    fn notify(&self, config_resp: Arc<ConfigResponse>) {
        let _ = self.0.send(config_resp);
    }
}

#[test]
fn fill_data_returns_err_when_content_missing() {
    let resp = response(None, Some("md5"));

    assert!(ConfigWorker::fill_data(&mut cache_data(), resp).is_err());
}

#[test]
fn fill_data_returns_err_when_md5_missing() {
    let resp = response(Some("content"), None);

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
    with_content.replace_snapshot("content".into(), "text".into(), "md5".into());

    assert_eq!(
        Some(true),
        ConfigWorker::clear_deleted(&mut with_content, "group-key")
    );
    assert!(with_content.snapshot.content().is_empty());
    assert!(with_content.snapshot.md5().is_empty());
    assert_eq!(
        None,
        ConfigWorker::clear_deleted(&mut with_content, "group-key")
    );

    let mut first_fill = cache_data();
    first_fill.initializing = true;
    first_fill.replace_snapshot("content".into(), "text".into(), "md5".into());

    assert_eq!(
        Some(false),
        ConfigWorker::clear_deleted(&mut first_fill, "group-key")
    );
    assert!(first_fill.snapshot.content().is_empty());
    assert!(first_fill.snapshot.md5().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn handle_config_not_found_holds_no_lock_across_filter_await() {
    use crate::api::plugin::{ConfigReq, ConfigResp};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        fn notify(&self, _: Arc<ConfigResponse>) {
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
    data.replace_snapshot("stale".into(), "text".into(), "stale-md5".into());
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

    tokio::time::timeout(Duration::from_secs(2), async {
        while !entered.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("deletion should reach the config filter");

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
    assert!(entry.snapshot.content().is_empty());
    assert!(entry.snapshot.md5().is_empty());
}

#[tokio::test]
async fn config_not_found_clears_stale_cache_and_notifies_once() {
    let (cache, root) = disk_cache("not-found").await;
    let group_key = "data+_+group+_+test-ns".to_string();
    let cache_file = root
        .join("config")
        .join(TEST_SERVER)
        .join("test-ns")
        .join(&group_key);
    let filters: Arc<Vec<Box<dyn ConfigFilter>>> = Arc::new(Vec::new());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut data = CacheData::new("data".into(), "group".into(), "test-ns".into());
    data.replace_snapshot("stale".into(), "text".into(), "stale-md5".into());
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
        assert_eq!(entry.snapshot.content(), "");
        assert_eq!(entry.snapshot.md5(), "");
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
async fn deletion_notifies_listener_that_never_saw_the_config() {
    let cache: Cache<CacheData> = CacheBuilder::config("test-ns".to_string()).build().await;
    let key = "data+_+group+_+test-ns".to_string();
    let filters: Arc<Vec<Box<dyn ConfigFilter>>> = Arc::new(Vec::new());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut data = CacheData::new("data".into(), "group".into(), "test-ns".into());
    data.replace_snapshot("stale".into(), "text".into(), "stale-md5".into());
    data.initializing = false;
    // The listener registers for an already-cached entry, so its last_md5 stays
    // empty until something is dispatched.
    data.add_listener(Arc::new(ChannelListener(tx)));
    cache.insert(key.clone(), data);

    ConfigWorker::handle_config_not_found(&cache, &key, &filters).await;

    let deleted = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("a subscribed listener must learn about the deletion")
        .expect("listener channel should stay open");
    assert_eq!(deleted.content(), "");
    assert_eq!(deleted.md5(), "");
}

#[tokio::test]
async fn startup_prune_removes_tombstones_and_their_files() {
    let (cache, root) = disk_cache("prune").await;
    let dir = root.join("config").join(TEST_SERVER).join("test-ns");

    let tombstone_key = "gone+_+group+_+test-ns".to_string();
    let tombstone = CacheData::new("gone".into(), "group".into(), "test-ns".into());
    cache.insert(tombstone_key.clone(), tombstone);

    let live_key = "live+_+group+_+test-ns".to_string();
    let mut live = CacheData::new("live".into(), "group".into(), "test-ns".into());
    live.replace_snapshot("value".into(), "text".into(), "m1".into());
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
async fn fill_cache_entry_persists_the_new_revision() {
    let (cache, root) = disk_cache("fill").await;
    let key = "data+_+group+_+test-ns".to_string();
    let cache_file = root
        .join("config")
        .join(TEST_SERVER)
        .join("test-ns")
        .join(&key);

    let mut data = CacheData::new("data".into(), "group".into(), "test-ns".into());
    data.replace_snapshot("old".into(), "text".into(), "old-md5".into());
    data.initializing = false;
    cache.insert(key.clone(), data);
    crate::test_config::wait_for_file(&cache_file, "initial insert should reach disk").await;

    assert!(ConfigWorker::fill_cache_entry(
        &cache,
        &key,
        response(Some("new"), Some("new-md5"))
    ));

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let bytes = tokio::fs::read(&cache_file)
                .await
                .expect("cache file should exist");
            if String::from_utf8_lossy(&bytes).contains("new-md5") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the filled revision should be persisted");

    assert_eq!(
        cache
            .get(&key)
            .expect("entry should exist")
            .snapshot
            .content(),
        "new"
    );

    let _ = tokio::fs::remove_dir_all(&root).await;
}

#[tokio::test]
async fn failed_initial_fill_does_not_swallow_next_notification() {
    let mut data = cache_data();
    let filters: Arc<Vec<Box<dyn ConfigFilter>>> = Arc::new(Vec::new());
    ConfigWorker::fill_data_and_notify(&mut data, response(None, None), &filters).await;

    assert!(
        ConfigWorker::fill_data(&mut data, response(Some("content"), Some("md5")))
            .expect("valid response should fill the cache"),
        "first successful fill after a rejected initial response must notify"
    );
}

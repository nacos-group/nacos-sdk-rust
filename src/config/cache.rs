use crate::api::config::ConfigResponse;
use crate::api::plugin::ConfigFilter;
use crate::api::plugin::ConfigResp;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

/// Cache Data for Config
#[derive(Serialize, Deserialize, Default)]
pub(crate) struct CacheData {
    pub data_id: Arc<String>,
    pub group: Arc<String>,
    pub namespace: Arc<String>,
    /// Default text; text, json, properties, html, xml, yaml ...
    pub content_type: Arc<String>,
    pub content: Arc<String>,
    pub md5: Arc<String>,
    /// whether content was encrypted with encryptedDataKey.
    pub encrypted_data_key: Arc<String>,
    pub last_modified: i64,

    /// There are some logical differences in the initialization phase, such as no notification of config changed
    #[serde(skip)]
    pub initializing: bool,

    /// who listen of config change. (runtime only, don't persist)
    #[serde(skip)]
    pub listeners: Arc<Mutex<Vec<ListenerWrapper>>>,
}

impl CacheData {
    pub fn new(data_id: String, group: String, namespace: String) -> Self {
        Self {
            data_id: Arc::new(data_id),
            group: Arc::new(group),
            namespace: Arc::new(namespace),
            content_type: Arc::new("text".to_string()),
            initializing: true,
            ..Default::default()
        }
    }

    /// Add listener.
    pub fn add_listener(&mut self, listener: Arc<dyn crate::api::config::ConfigChangeListener>) {
        if let Ok(mut mutex) = self.listeners.lock() {
            if Self::index_of_listener(mutex.deref(), &listener).is_some() {
                return;
            }
            mutex.push(ListenerWrapper::new(Arc::clone(&listener)));
        }
    }

    /// Remove listener.
    pub fn remove_listener(&mut self, listener: Arc<dyn crate::api::config::ConfigChangeListener>) {
        if let Ok(mut mutex) = self.listeners.lock()
            && let Some(idx) = Self::index_of_listener(mutex.deref(), &listener)
        {
            mutex.swap_remove(idx);
        }
    }

    /// fn inner, return idx if existed, else return None.
    fn index_of_listener(
        listen_warp_vec: &[ListenerWrapper],
        listener: &Arc<dyn crate::api::config::ConfigChangeListener>,
    ) -> Option<usize> {
        listen_warp_vec
            .iter()
            .position(|listen_warp| Arc::ptr_eq(&listen_warp.listener, listener))
    }

    /// Notify listener. when last-md5 not equals the-newest-md5
    ///
    /// NOTE: never call while holding a DashMap guard — it awaits (config filters),
    /// and a guard held across an await can deadlock the SDK runtime.
    pub async fn notify_listener(&mut self, config_filters: &Arc<Vec<Box<dyn ConfigFilter>>>) {
        let config_resp = self.resp_source(config_filters).into_response().await;
        self.dispatch_notify(config_resp);
    }

    /// Dispatch notification synchronously: compare last_md5 and notify in an
    /// independent task. Never awaits, safe to call while holding a lock.
    pub(crate) fn dispatch_notify(&self, config_resp: ConfigResponse) {
        tracing::info!(
            "dispatch_notify, dataId={},group={},namespace={},md5={}",
            self.data_id,
            self.group,
            self.namespace,
            self.md5
        );

        if let Ok(mut mutex) = self.listeners.lock() {
            for listen_wrap in mutex.iter_mut() {
                if listen_wrap.last_md5.as_str() == self.md5.as_str() {
                    continue;
                }
                // Notify when last-md5 not equals the-newest-md5, Notify in independent thread.
                let l_clone = listen_wrap.listener.clone();
                let c_clone = config_resp.clone();
                crate::common::executor::spawn(async move {
                    l_clone.notify(c_clone);
                });
                listen_wrap.last_md5 = Arc::clone(&self.md5);
            }
        }
    }

    /// Build a response source that can be materialized without holding a cache guard.
    ///
    /// Filters belong to the worker exit, not to the cached data, so entries
    /// restored from disk behave like entries created at runtime.
    pub(crate) fn resp_source(
        &self,
        config_filters: &Arc<Vec<Box<dyn ConfigFilter>>>,
    ) -> RespSource {
        if config_filters.is_empty() {
            RespSource::Raw(self.raw_response())
        } else {
            RespSource::Filtered(self.resp_snapshot(config_filters))
        }
    }

    /// Build a response sharing the cached strings.
    pub(crate) fn raw_response(&self) -> ConfigResponse {
        ConfigResponse::from_shared(
            Arc::clone(&self.data_id),
            Arc::clone(&self.group),
            Arc::clone(&self.namespace),
            Arc::clone(&self.content),
            Arc::clone(&self.content_type),
            Arc::clone(&self.md5),
        )
    }

    /// Clone a pre-filter response snapshot, so async filters can run lock-free.
    pub(crate) fn resp_snapshot(
        &self,
        config_filters: &Arc<Vec<Box<dyn ConfigFilter>>>,
    ) -> RespSnapshot {
        (
            Arc::clone(config_filters),
            ConfigResp::new(
                self.data_id.to_string(),
                self.group.to_string(),
                self.namespace.to_string(),
                self.content.to_string(),
                self.encrypted_data_key.to_string(),
            ),
            Arc::clone(&self.content_type),
            Arc::clone(&self.md5),
        )
    }

    /// Apply async config filters to a snapshot. No DashMap guard may be alive.
    pub(crate) async fn filtered_response(
        (filters, mut conf_resp, content_type, md5): RespSnapshot,
    ) -> ConfigResponse {
        for config_filter in filters.iter() {
            config_filter.filter(None, Some(&mut conf_resp)).await;
        }

        ConfigResponse::new(
            conf_resp.data_id,
            conf_resp.group,
            conf_resp.namespace,
            conf_resp.content,
            content_type.as_ref().clone(),
            md5.as_ref().clone(),
        )
    }
}

pub(crate) enum RespSource {
    Raw(ConfigResponse),
    Filtered(RespSnapshot),
}

impl RespSource {
    pub(crate) async fn into_response(self) -> ConfigResponse {
        match self {
            RespSource::Raw(response) => response,
            RespSource::Filtered(snapshot) => CacheData::filtered_response(snapshot).await,
        }
    }
}

impl std::fmt::Display for CacheData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CacheData(namespace={n},data_id={d},group={g},md5={m},encrypted_data_key={k},content_type={t},content=",
            n = self.namespace,
            d = self.data_id,
            g = self.group,
            m = self.md5,
            k = self.encrypted_data_key,
            t = self.content_type,
        )?;
        // Truncate content for display if it exceeds 30 chars
        if self.content.chars().count() > 30 {
            for c in self.content.chars().take(30) {
                write!(f, "{}", c)?;
            }
            write!(f, "...")?;
        } else {
            write!(f, "{}", self.content)?;
        }
        write!(f, ")")
    }
}

/// Pre-filter response snapshot: (filters, raw resp, content_type, md5).
pub(crate) type RespSnapshot = (
    Arc<Vec<Box<dyn ConfigFilter>>>,
    ConfigResp,
    Arc<String>,
    Arc<String>,
);

/// The inner Wrapper of ConfigChangeListener
pub(crate) struct ListenerWrapper {
    /// last md5 be notified
    last_md5: Arc<String>,
    listener: Arc<dyn crate::api::config::ConfigChangeListener>,
}

impl ListenerWrapper {
    fn new(listener: Arc<dyn crate::api::config::ConfigChangeListener>) -> Self {
        Self {
            last_md5: Arc::new(String::new()),
            listener,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RespSource;
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use crate::config::cache::CacheData;
    use std::sync::Arc;

    #[test]
    fn test_cache_data_add_listener() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let mut cache_data = CacheData::new(d, g, n);

        // test add listener1
        let lis1_arc = Arc::new(TestConfigChangeListener1 {});
        cache_data.add_listener(lis1_arc);

        // test add listener2
        let lis2_arc = Arc::new(TestConfigChangeListener2 {});
        cache_data.add_listener(lis2_arc.clone());
        // test add a listener2 again
        cache_data.add_listener(lis2_arc);

        let listen_mutex = cache_data
            .listeners
            .lock()
            .expect("mutex should not be poisoned");
        assert_eq!(2, listen_mutex.len());
    }

    #[test]
    fn test_cache_data_add_listener_then_remove() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let mut cache_data = CacheData::new(d, g, n);

        // test add listener1
        let lis1_arc = Arc::new(TestConfigChangeListener1 {});
        let lis1_arc2 = Arc::clone(&lis1_arc);
        cache_data.add_listener(lis1_arc);

        // test add listener2
        let lis2_arc = Arc::new(TestConfigChangeListener2 {});
        let lis2_arc2 = Arc::clone(&lis2_arc);
        cache_data.add_listener(lis2_arc);
        {
            let listen_mutex = cache_data
                .listeners
                .lock()
                .expect("mutex should not be poisoned");
            assert_eq!(2, listen_mutex.len());
        }

        cache_data.remove_listener(lis1_arc2);
        {
            let listen_mutex = cache_data
                .listeners
                .lock()
                .expect("mutex should not be poisoned");
            assert_eq!(1, listen_mutex.len());
        }
        cache_data.remove_listener(lis2_arc2);
        {
            let listen_mutex = cache_data
                .listeners
                .lock()
                .expect("mutex should not be poisoned");
            assert_eq!(0, listen_mutex.len());
        }
    }

    #[test]
    fn test_cache_data_serde_roundtrip() {
        let mut data = CacheData::new("D".to_string(), "G".to_string(), "N".to_string());
        data.content = Arc::new("hello".to_string());
        data.md5 = Arc::new("m1".to_string());
        data.content_type = Arc::new("json".to_string());
        data.encrypted_data_key = Arc::new("key".to_string());
        data.last_modified = 42;

        let json = serde_json::to_string(&data).expect("cache data should serialize");
        assert!(json.contains("\"content\":\"hello\""));
        assert!(!json.contains("listeners"));
        assert!(!json.contains("initializing"));

        let decoded: CacheData =
            serde_json::from_str(&json).expect("cache data should deserialize");
        assert_eq!(decoded.data_id.as_str(), "D");
        assert_eq!(decoded.group.as_str(), "G");
        assert_eq!(decoded.namespace.as_str(), "N");
        assert_eq!(decoded.content.as_str(), "hello");
        assert_eq!(decoded.content_type.as_str(), "json");
        assert_eq!(decoded.md5.as_str(), "m1");
        assert_eq!(decoded.encrypted_data_key.as_str(), "key");
        assert_eq!(decoded.last_modified, 42);
    }

    #[test]
    fn test_deserialize_legacy_cache_json() {
        // Fixture written by 0.8.4: no runtime fields, plain string values.
        let json = r#"{"data_id":"D","group":"G","namespace":"N","content_type":"text","content":"hello","md5":"m1","encrypted_data_key":"","last_modified":42}"#;
        let decoded: CacheData =
            serde_json::from_str(json).expect("legacy cache json should deserialize");

        assert_eq!(decoded.data_id.as_str(), "D");
        assert_eq!(decoded.group.as_str(), "G");
        assert_eq!(decoded.namespace.as_str(), "N");
        assert_eq!(decoded.content.as_str(), "hello");
        assert_eq!(decoded.content_type.as_str(), "text");
        assert_eq!(decoded.md5.as_str(), "m1");
        assert_eq!(decoded.last_modified, 42);
    }

    #[tokio::test]
    async fn test_loaded_entry_uses_worker_filters() {
        use crate::api::plugin::{ConfigFilter, ConfigReq, ConfigResp};

        struct Append(&'static str);

        #[async_trait::async_trait]
        impl ConfigFilter for Append {
            async fn filter(
                &self,
                _: Option<&mut ConfigReq>,
                config_resp: Option<&mut ConfigResp>,
            ) {
                if let Some(config_resp) = config_resp {
                    config_resp.content.push_str(self.0);
                }
            }
        }

        let json = r#"{"data_id":"D","group":"G","namespace":"N","content_type":"text","content":"hello","md5":"m1","encrypted_data_key":"","last_modified":42}"#;
        let data: CacheData =
            serde_json::from_str(json).expect("legacy cache json should deserialize");
        let filters: Arc<Vec<Box<dyn ConfigFilter>>> =
            Arc::new(vec![Box::new(Append("-decrypted"))]);

        let response = data.resp_source(&filters).into_response().await;
        assert_eq!(response.content(), "hello-decrypted");
    }

    #[test]
    fn test_resp_source_without_filters_is_raw_and_shared() {
        let mut data = CacheData::new("D".to_string(), "G".to_string(), "N".to_string());
        data.content = Arc::new("hello".to_string());

        let RespSource::Raw(response) = data.resp_source(&Arc::new(Vec::new())) else {
            panic!("a cache entry without filters must take the raw path");
        };
        assert!(response.shares_content_with(&data.content));
    }

    #[tokio::test]
    async fn test_resp_source_applies_filters_in_order() {
        use crate::api::plugin::{ConfigFilter, ConfigReq, ConfigResp};

        struct Append(&'static str);

        #[async_trait::async_trait]
        impl ConfigFilter for Append {
            async fn filter(
                &self,
                _: Option<&mut ConfigReq>,
                config_resp: Option<&mut ConfigResp>,
            ) {
                if let Some(config_resp) = config_resp {
                    config_resp.content.push_str(self.0);
                }
            }
        }

        let filters: Arc<Vec<Box<dyn ConfigFilter>>> =
            Arc::new(vec![Box::new(Append("-1")), Box::new(Append("-2"))]);
        let mut data = CacheData::new("D".to_string(), "G".to_string(), "N".to_string());
        data.content = Arc::new("base".to_string());

        let response = data.resp_source(&filters).into_response().await;
        assert_eq!(response.content(), "base-1-2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dispatch_notify_is_md5_deduplicated() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        struct CountingListener(Arc<AtomicUsize>);

        impl ConfigChangeListener for CountingListener {
            fn notify(&self, _: ConfigResponse) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let hits = Arc::new(AtomicUsize::new(0));
        let mut data = CacheData::new("D".to_string(), "G".to_string(), "N".to_string());
        data.md5 = Arc::new("m1".to_string());
        data.add_listener(Arc::new(CountingListener(hits.clone())));

        data.dispatch_notify(data.raw_response());
        data.dispatch_notify(data.raw_response());
        data.md5 = Arc::new("m2".to_string());
        data.dispatch_notify(data.raw_response());

        tokio::time::timeout(Duration::from_secs(2), async {
            while hits.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("listeners should be notified");

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(2, hits.load(Ordering::SeqCst));
    }

    struct TestConfigChangeListener1;
    struct TestConfigChangeListener2;

    impl ConfigChangeListener for TestConfigChangeListener1 {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::info!(
                "TestConfigChangeListener1 listen the config={}",
                config_resp
            );
        }
    }

    impl ConfigChangeListener for TestConfigChangeListener2 {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::info!(
                "TestConfigChangeListener2 listen the config={}",
                config_resp
            );
        }
    }

    /// The notify flow must never hold a DashMap guard across an await.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_notify_flow_holds_no_lock_across_await() {
        use crate::api::plugin::{ConfigFilter, ConfigReq, ConfigResp};
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

        struct CountingListener {
            hits: AtomicUsize,
        }

        impl ConfigChangeListener for CountingListener {
            fn notify(&self, _: ConfigResponse) {
                self.hits.fetch_add(1, Ordering::SeqCst);
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
        let key = "D+G+test-ns".to_string();
        let listener = Arc::new(CountingListener {
            hits: AtomicUsize::new(0),
        });

        let mut data = CacheData::new("D".to_string(), "G".to_string(), "test-ns".to_string());
        data.content = Arc::new("c".to_string());
        data.md5 = Arc::new("m1".to_string());
        data.initializing = false;
        data.add_listener(listener.clone());
        cache.insert(key.clone(), data);

        // Mirror the worker's notify path: snapshot under a short read guard,
        // run async filters lock-free, then dispatch synchronously.
        let cache_in_task = Arc::clone(&cache);
        let key_in_task = key.clone();
        let filters_in_task = Arc::clone(&filters);
        let notify_task = tokio::spawn(async move {
            let snapshot = cache_in_task
                .get(&key_in_task)
                .map(|r| r.resp_snapshot(&filters_in_task))
                .expect("entry should exist");
            let resp = CacheData::filtered_response(snapshot).await;
            if let Some(r) = cache_in_task.get(&key_in_task) {
                r.dispatch_notify(resp);
            }
        });

        // Wait until the filter is in-flight; the read guard must already be dropped.
        while !entered.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }

        // While the notify flow is suspended, the entry must stay writable...
        tokio::time::timeout(Duration::from_millis(200), async {
            let mut w = cache.get_mut(&key).expect("entry should exist");
            w.last_modified = 42;
        })
        .await
        .expect("get_mut blocked: a cache lock is held across an await");

        // ...and iterable (this is what list_ensure_cache_data_newest does).
        tokio::time::timeout(Duration::from_millis(200), async {
            let mut n = 0;
            cache.for_each(|_, _| n += 1);
            assert_eq!(1, n);
        })
        .await
        .expect("for_each blocked: a cache lock is held across an await");

        // Release the filter; the listener must be notified and last_md5 updated.
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), notify_task)
            .await
            .expect("notify task stuck")
            .expect("notify task panicked");

        tokio::time::timeout(Duration::from_secs(2), async {
            while listener.hits.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("listener was not notified");

        let r = cache.get(&key).expect("entry should exist");
        let last_md5 = r.listeners.lock().expect("mutex should not be poisoned")[0]
            .last_md5
            .clone();
        assert_eq!("m1", last_md5.as_str());
    }

    /// A write guard held across an await blocks all access to the shard.
    /// Pinned here as the failure mode the notify flow must avoid.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_guard_across_await_blocks_cache() {
        use crate::common::cache::{Cache, CacheBuilder};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        let cache: Arc<Cache<CacheData>> =
            Arc::new(CacheBuilder::config("test-ns".to_string()).build().await);
        let key = "D+G+test-ns".to_string();
        cache.insert(
            key.clone(),
            CacheData::new("D".to_string(), "G".to_string(), "test-ns".to_string()),
        );

        let entered = Arc::new(AtomicBool::new(false));
        let release = Arc::new(tokio::sync::Notify::new());
        let holder = {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            tokio::spawn(async move {
                let _guard = cache.get_mut(&key); // guard held...
                entered.store(true, Ordering::Release);
                release.notified().await; // ...across an await
            })
        };

        while !entered.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }

        // Iterating the shard must now block. Runs on the blocking pool so the
        // test's own single worker stays free to time out and recover.
        let iter = {
            let cache = Arc::clone(&cache);
            tokio::task::spawn_blocking(move || cache.for_each(|_, _| {}))
        };
        let blocked = tokio::time::timeout(Duration::from_millis(200), iter)
            .await
            .is_err();
        assert!(
            blocked,
            "for_each should block while a write guard is held across an await"
        );

        // After the guard drops, iteration recovers.
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), holder)
            .await
            .expect("guard holder stuck")
            .expect("guard holder panicked");
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut n = 0;
            cache.for_each(|_, _| n += 1);
            assert_eq!(1, n);
        })
        .await
        .expect("for_each still blocked after guard dropped");
    }
}

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::ai::message::is_not_found;
use crate::ai::message::response::QueryMcpServerResponse;
use crate::ai::model::mcp::McpServerDetailInfo;
use crate::api::error::{Error, Result};
use crate::common::error::handle_response;
use crate::common::executor;

const LATEST_VERSION: &str = "latest";

fn versioned_key(name: &str, version: Option<&str>) -> String {
    match version.map(str::trim).filter(|value| !value.is_empty()) {
        Some(version) => format!("{name}::{version}"),
        None => format!("{name}::{LATEST_VERSION}"),
    }
}

/// The production query converts the structured response here so that a 404
/// surfaces as `ErrResponse(_, _, 404, _)`; `is_not_found` matches that variant.
pub(crate) fn query_result(response: QueryMcpServerResponse) -> Result<McpServerDetailInfo> {
    handle_response(&response)?;
    response
        .mcp_server_detail_info
        .ok_or_else(|| Error::ErrResult("mcp server query returned no detail".to_string()))
}

#[derive(Default)]
struct McpServerCache {
    details: HashMap<String, Arc<McpServerDetailInfo>>,
}

impl McpServerCache {
    #[cfg(test)]
    fn get(&self, key: &str) -> Option<McpServerDetailInfo> {
        self.details.get(key).map(|detail| detail.as_ref().clone())
    }

    /// Store the last detail seen for one subscription key. Returns whether it changed.
    fn upsert(&mut self, key: &str, detail: &Arc<McpServerDetailInfo>) -> bool {
        let changed = self
            .details
            .get(key)
            .is_none_or(|stored| stored.as_ref() != detail.as_ref());
        if changed {
            self.details.insert(key.to_owned(), Arc::clone(detail));
        }
        changed
    }

    fn remove(&mut self, key: &str) {
        self.details.remove(key);
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.details.is_empty()
    }
}

/// Receives MCP server changes for a subscription.
pub trait McpServerListener: Send + Sync {
    fn notify(&self, mcp_server: Arc<McpServerDetailInfo>);
}

#[async_trait]
pub(crate) trait McpServerQuery: Send + Sync {
    async fn query(&self, name: &str, version: Option<&str>) -> Result<McpServerDetailInfo>;
}

#[derive(Default)]
struct Subscription {
    listeners: Vec<Arc<dyn McpServerListener>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

/// Keeps one poller per subscribed `name`/`version` pair and fans changes out to listeners.
pub(crate) struct McpServerUpdater {
    namespace: String,
    query: Arc<dyn McpServerQuery>,
    interval: Duration,
    cache: Mutex<McpServerCache>,
    subscriptions: Mutex<HashMap<String, Subscription>>,
}

impl McpServerUpdater {
    pub(crate) fn new(
        namespace: String,
        query: Arc<dyn McpServerQuery>,
        interval: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            namespace,
            query,
            interval,
            cache: Mutex::new(McpServerCache::default()),
            subscriptions: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) async fn subscribe(
        self: &Arc<Self>,
        name: &str,
        version: Option<&str>,
        listener: Arc<dyn McpServerListener>,
    ) -> Result<Option<Arc<McpServerDetailInfo>>> {
        let version = version.map(str::trim).filter(|value| !value.is_empty());
        let key = versioned_key(name, version);

        let detail = match self.query.query(name, version).await {
            Ok(detail) => Some(detail),
            Err(error) if is_not_found(&error) => {
                warn!(
                    namespace = %self.namespace,
                    mcp_name = name,
                    version = version.unwrap_or(LATEST_VERSION),
                    "mcp server not found while subscribing; polling continues"
                );
                None
            }
            Err(error) => return Err(error),
        };
        self.register_listener(&key, name, version, listener).await;
        match detail {
            Some(detail) => Ok(Some(self.apply(&key, detail).await)),
            None => Ok(None),
        }
    }

    pub(crate) async fn unsubscribe(
        &self,
        name: &str,
        version: Option<&str>,
        listener: &Arc<dyn McpServerListener>,
    ) -> Result<()> {
        let key = versioned_key(name, version);
        let mut subscriptions = self.subscriptions.lock().await;
        let empty = subscriptions.get_mut(&key).is_some_and(|subscription| {
            subscription
                .listeners
                .retain(|entry| !Arc::ptr_eq(entry, listener));
            subscription.listeners.is_empty()
        });
        if empty {
            if let Some(subscription) = subscriptions.remove(&key)
                && let Some(task) = subscription.task
            {
                task.abort();
            }
            self.cache.lock().await.remove(&key);
        }
        Ok(())
    }

    async fn register_listener(
        self: &Arc<Self>,
        key: &str,
        name: &str,
        version: Option<&str>,
        listener: Arc<dyn McpServerListener>,
    ) {
        let mut subscriptions = self.subscriptions.lock().await;
        let subscription = subscriptions.entry(key.to_owned()).or_default();
        if !subscription
            .listeners
            .iter()
            .any(|entry| Arc::ptr_eq(entry, &listener))
        {
            subscription.listeners.push(listener);
        }
        if subscription.task.is_none() {
            subscription.task = Some(self.spawn_poller(name, version));
        }
    }

    fn spawn_poller(
        self: &Arc<Self>,
        name: &str,
        version: Option<&str>,
    ) -> tokio::task::JoinHandle<()> {
        let interval = self.interval;
        let key = versioned_key(name, version);
        let weak = Arc::downgrade(self);
        let name = name.to_string();
        let version = version.map(str::to_string);
        executor::spawn(async move {
            loop {
                sleep(interval).await;
                let Some(updater) = weak.upgrade() else {
                    debug!(mcp_name = %name, "mcp subscription poller dropped with its updater");
                    break;
                };
                match updater.query.query(&name, version.as_deref()).await {
                    Ok(detail) => {
                        updater.apply(&key, detail).await;
                    }
                    Err(error) if is_not_found(&error) => {
                        warn!(
                            namespace = %updater.namespace,
                            mcp_name = %name,
                            version = version.as_deref().unwrap_or(LATEST_VERSION),
                            "mcp server disappeared while polling; keeping cached value"
                        );
                    }
                    Err(error) => {
                        warn!(
                            namespace = %updater.namespace,
                            mcp_name = %name,
                            version = version.as_deref().unwrap_or(LATEST_VERSION),
                            "mcp subscription poll failed: {error}"
                        );
                    }
                }
            }
        })
    }

    async fn apply(&self, key: &str, detail: McpServerDetailInfo) -> Arc<McpServerDetailInfo> {
        let detail = Arc::new(detail);
        let changed = {
            let subscriptions = self.subscriptions.lock().await;
            if !subscriptions.contains_key(key) {
                return detail;
            }
            self.cache.lock().await.upsert(key, &detail)
        };
        if changed {
            self.dispatch(detail.clone()).await;
        }
        detail
    }

    async fn dispatch(&self, detail: Arc<McpServerDetailInfo>) {
        let Some(name) = detail.name.as_deref() else {
            return;
        };
        // Latest details route to the `latest` key, matching the Java client.
        let event_key = match detail.version_detail.as_ref() {
            Some(version) if version.is_latest == Some(true) => versioned_key(name, None),
            Some(version) => versioned_key(name, Some(&version.version)),
            None => return,
        };
        let listeners = {
            let subscriptions = self.subscriptions.lock().await;
            let Some(subscription) = subscriptions.get(&event_key) else {
                return;
            };
            if subscription.listeners.is_empty() {
                return;
            }
            subscription.listeners.clone()
        };
        for listener in listeners {
            let detail = detail.clone();
            drop(executor::spawn_blocking(move || listener.notify(detail)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(version: &str, is_latest: Option<bool>, description: &str) -> McpServerDetailInfo {
        serde_json::from_value(serde_json::json!({
            "name": "svc",
            "versionDetail": {
                "version": version,
                "is_latest": is_latest,
                "release_date": "2026-11-16T04:30:50Z"
            },
            "description": description,
            "enabled": true
        }))
        .expect("detail should parse")
    }

    #[test]
    fn test_versioned_key_normalizes_empty_version_to_latest() {
        assert_eq!(versioned_key("svc", None), "svc::latest");
        assert_eq!(versioned_key("svc", Some("")), "svc::latest");
        assert_eq!(versioned_key("svc", Some("  ")), "svc::latest");
        assert_eq!(versioned_key("svc", Some("1.0.0")), "svc::1.0.0");
    }

    #[test]
    fn test_upsert_detects_first_and_real_changes() {
        let mut cache = McpServerCache::default();
        let key = "svc::latest";
        assert!(cache.upsert(key, &Arc::new(detail("1.0.0", Some(true), "first"))));
        assert!(!cache.upsert(key, &Arc::new(detail("1.0.0", Some(true), "first"))));
        assert!(cache.upsert(key, &Arc::new(detail("1.0.0", Some(true), "second"))));
    }

    #[test]
    fn test_upsert_tracks_each_subscription_key_independently() {
        let mut cache = McpServerCache::default();
        let latest = Arc::new(detail("1.0.0", Some(true), "latest"));
        let exact = Arc::new(detail("1.0.0", Some(false), "exact"));

        assert!(cache.upsert("svc::latest", &latest));
        assert!(cache.upsert("svc::1.0.0", &exact));
        assert_eq!(
            cache
                .get("svc::latest")
                .expect("latest key")
                .description
                .as_deref(),
            Some("latest")
        );
        assert_eq!(
            cache
                .get("svc::1.0.0")
                .expect("exact key")
                .description
                .as_deref(),
            Some("exact")
        );
        assert!(!cache.upsert("svc::latest", &latest));
    }

    #[test]
    fn test_not_found_matches_query_conversion() {
        let error = query_result(QueryMcpServerResponse {
            result_code: 500,
            error_code: 404,
            message: Some("mcp server not found".to_string()),
            ..Default::default()
        })
        .expect_err("a 404 response must be an error");
        assert!(is_not_found(&error));

        let other = query_result(QueryMcpServerResponse {
            result_code: 500,
            error_code: 500,
            message: Some("boom".to_string()),
            ..Default::default()
        })
        .expect_err("a 500 response must be an error");
        assert!(!is_not_found(&other));
    }

    #[test]
    fn test_upsert_is_stable_for_hashmap_key_order() {
        // HashMaps use a random per-instance seed, so repeat the construction to
        // make a value comparison that misses map-order differences fail reliably.
        for _ in 0..64 {
            let first: McpServerDetailInfo = serde_json::from_value(serde_json::json!({
                "name": "svc",
                "versionDetail": {"version": "1.0.0", "is_latest": true},
                "toolSpec": {"toolsMeta": {"b": {"enabled": true}, "a": {"enabled": true}}},
                "enabled": true
            }))
            .expect("first detail should parse");
            let second: McpServerDetailInfo = serde_json::from_value(serde_json::json!({
                "name": "svc",
                "versionDetail": {"version": "1.0.0", "is_latest": true},
                "toolSpec": {"toolsMeta": {"a": {"enabled": true}, "b": {"enabled": true}}},
                "enabled": true
            }))
            .expect("second detail should parse");
            assert_eq!(first, second);
            let first = Arc::new(first);
            let second = Arc::new(second);
            let mut cache = McpServerCache::default();
            assert!(cache.upsert("svc::latest", &first));
            assert!(!cache.upsert("svc::latest", &second));
        }
    }

    #[tokio::test]
    async fn test_last_unsubscribe_evicts_cache_entries() {
        let query = Arc::new(FakeQuery::new("first"));
        let updater = McpServerUpdater::new("public".to_string(), query, Duration::from_secs(10));
        let (tx, _rx) = mpsc::unbounded_channel();
        let listener: Arc<dyn McpServerListener> = Arc::new(ChannelListener { tx });

        updater
            .subscribe("svc", None, listener.clone())
            .await
            .expect("subscribe should succeed");
        assert!(
            !updater.cache.lock().await.is_empty(),
            "subscription should cache the last-seen detail"
        );

        updater
            .unsubscribe("svc", None, &listener)
            .await
            .expect("unsubscribe should succeed");
        assert!(
            updater.cache.lock().await.is_empty(),
            "the last unsubscribe should evict the cache"
        );
    }

    #[tokio::test]
    async fn test_apply_does_not_reinsert_an_unsubscribed_key() {
        let query = Arc::new(FakeQuery::new("first"));
        let updater = McpServerUpdater::new("public".to_string(), query, Duration::from_secs(10));
        let (tx, _rx) = mpsc::unbounded_channel();
        let listener: Arc<dyn McpServerListener> = Arc::new(ChannelListener { tx });

        updater
            .subscribe("svc", None, listener.clone())
            .await
            .expect("subscribe should succeed");
        updater
            .unsubscribe("svc", None, &listener)
            .await
            .expect("unsubscribe should succeed");

        updater
            .apply("svc::latest", detail("1.0.0", Some(true), "late poll"))
            .await;
        assert!(
            updater.cache.lock().await.is_empty(),
            "a poll completing after unsubscribe must not repopulate the cache"
        );
    }

    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use crate::api::error::Result;

    struct FakeQuery {
        description: std::sync::Mutex<String>,
    }

    impl FakeQuery {
        fn new(description: &str) -> Self {
            Self {
                description: std::sync::Mutex::new(description.to_string()),
            }
        }

        fn set(&self, description: &str) {
            *self.description.lock().expect("lock") = description.to_string();
        }
    }

    #[async_trait]
    impl McpServerQuery for FakeQuery {
        async fn query(&self, _name: &str, _version: Option<&str>) -> Result<McpServerDetailInfo> {
            let description = self.description.lock().expect("lock").clone();
            Ok(detail("1.0.0", Some(true), &description))
        }
    }

    struct ChannelListener {
        tx: mpsc::UnboundedSender<Arc<McpServerDetailInfo>>,
    }

    impl McpServerListener for ChannelListener {
        fn notify(&self, mcp_server: Arc<McpServerDetailInfo>) {
            let _ = self.tx.send(mcp_server);
        }
    }

    #[tokio::test]
    async fn test_subscribe_notifies_initial_value_and_changes() {
        let query = Arc::new(FakeQuery::new("first"));
        let updater = McpServerUpdater::new(
            "public".to_string(),
            query.clone(),
            Duration::from_millis(50),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        let listener: Arc<dyn McpServerListener> = Arc::new(ChannelListener { tx });

        let initial = updater
            .subscribe("svc", None, listener.clone())
            .await
            .expect("subscribe should succeed");
        assert_eq!(
            initial.expect("initial value").description.as_deref(),
            Some("first")
        );
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("initial notification")
            .expect("channel open");
        assert_eq!(first.description.as_deref(), Some("first"));

        query.set("second");
        let changed = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("change notification")
            .expect("channel open");
        assert_eq!(changed.description.as_deref(), Some("second"));

        updater
            .unsubscribe("svc", None, &listener)
            .await
            .expect("unsubscribe should succeed");
        query.set("third");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            rx.try_recv().is_err(),
            "listener must stop after unsubscribe"
        );
    }

    #[tokio::test]
    async fn test_duplicate_subscription_notifies_once_per_change() {
        let query = Arc::new(FakeQuery::new("first"));
        let updater = McpServerUpdater::new(
            "public".to_string(),
            query.clone(),
            Duration::from_millis(50),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        let listener: Arc<dyn McpServerListener> = Arc::new(ChannelListener { tx });

        updater
            .subscribe("svc", None, listener.clone())
            .await
            .expect("first subscribe should succeed");
        let initial = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("initial notification")
            .expect("channel open");
        assert_eq!(initial.description.as_deref(), Some("first"));

        updater
            .subscribe("svc", None, listener.clone())
            .await
            .expect("duplicate subscribe should succeed");

        query.set("second");
        let changed = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("change notification")
            .expect("channel open");
        assert_eq!(changed.description.as_deref(), Some("second"));

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            rx.try_recv().is_err(),
            "a duplicate registration must not double-notify"
        );

        updater
            .unsubscribe("svc", None, &listener)
            .await
            .expect("unsubscribe should succeed");
    }

    #[tokio::test]
    async fn test_subscribe_returns_none_when_server_missing() {
        struct NotFoundQuery;

        #[async_trait]
        impl McpServerQuery for NotFoundQuery {
            async fn query(
                &self,
                _name: &str,
                _version: Option<&str>,
            ) -> Result<McpServerDetailInfo> {
                query_result(QueryMcpServerResponse {
                    result_code: 500,
                    error_code: 404,
                    message: Some("mcp server not found".to_string()),
                    ..Default::default()
                })
            }
        }

        let updater = McpServerUpdater::new(
            "public".to_string(),
            Arc::new(NotFoundQuery),
            Duration::from_secs(60),
        );
        let (tx, _rx) = mpsc::unbounded_channel();
        let value = updater
            .subscribe("missing", None, Arc::new(ChannelListener { tx }))
            .await
            .expect("NOT_FOUND must not fail the subscribe call");
        assert!(value.is_none());
    }
}

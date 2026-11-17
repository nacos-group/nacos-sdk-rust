use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::ai::message::NOT_FOUND_CODE;
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

fn is_not_found(error: &Error) -> bool {
    matches!(error, Error::ErrResponse(_, _, NOT_FOUND_CODE, _))
}

#[derive(Default)]
struct McpServerCache {
    canonical: HashMap<String, Arc<str>>,
}

impl McpServerCache {
    #[cfg(test)]
    fn get(&self, name: &str, version: Option<&str>) -> Option<McpServerDetailInfo> {
        self.canonical
            .get(&versioned_key(name, version))
            .and_then(|json| serde_json::from_str(json.as_ref()).ok())
    }

    /// Store `detail` under its exact version key and, when it is the latest
    /// version, under the `latest` alias. Returns whether any stored key changed.
    fn upsert(&mut self, detail: &McpServerDetailInfo) -> bool {
        let Some(name) = detail.name.as_deref() else {
            return false;
        };
        let Some(version_detail) = detail.version_detail.as_ref() else {
            debug!(
                mcp_name = name,
                "mcp server detail without version_detail; skipping cache update"
            );
            return false;
        };
        let json =
            match serde_json::to_value(detail).and_then(|value| serde_json::to_string(&value)) {
                Ok(json) => json,
                Err(error) => {
                    warn!(
                        mcp_name = name,
                        version = %version_detail.version,
                        "mcp server detail serialization failed; dropping cache update: {error}"
                    );
                    return false;
                }
            };
        let json: Arc<str> = Arc::from(json);
        let mut changed = self.store(versioned_key(name, Some(&version_detail.version)), &json);
        if version_detail.is_latest == Some(true) {
            changed |= self.store(versioned_key(name, None), &json);
        }
        changed
    }

    fn store(&mut self, key: String, json: &Arc<str>) -> bool {
        let changed = self.canonical.get(&key).map(|value| value.as_ref()) != Some(json.as_ref());
        if changed {
            self.canonical.insert(key, Arc::clone(json));
        }
        changed
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

        match self.query.query(name, version).await {
            Ok(detail) => {
                self.register_listener(key, name, version, listener).await;
                Ok(Some(self.apply(detail).await))
            }
            Err(error) if is_not_found(&error) => {
                self.register_listener(key, name, version, listener).await;
                warn!(
                    namespace = %self.namespace,
                    mcp_name = name,
                    version = version.unwrap_or(LATEST_VERSION),
                    "mcp server not found while subscribing; polling continues"
                );
                Ok(None)
            }
            Err(error) => Err(error),
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
        if empty
            && let Some(subscription) = subscriptions.remove(&key)
            && let Some(task) = subscription.task
        {
            task.abort();
        }
        Ok(())
    }

    async fn register_listener(
        self: &Arc<Self>,
        key: String,
        name: &str,
        version: Option<&str>,
        listener: Arc<dyn McpServerListener>,
    ) {
        let mut subscriptions = self.subscriptions.lock().await;
        let subscription = subscriptions.entry(key).or_default();
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
                        updater.apply(detail).await;
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

    async fn apply(&self, detail: McpServerDetailInfo) -> Arc<McpServerDetailInfo> {
        let detail = Arc::new(detail);
        let changed = self.cache.lock().await.upsert(&detail);
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
        assert!(cache.upsert(&detail("1.0.0", Some(true), "first")));
        assert!(!cache.upsert(&detail("1.0.0", Some(true), "first")));
        assert!(cache.upsert(&detail("1.0.0", Some(true), "second")));
    }

    #[test]
    fn test_upsert_writes_latest_alias_only_for_latest_versions() {
        let mut cache = McpServerCache::default();
        cache.upsert(&detail("1.0.0", Some(true), "first"));
        assert!(cache.get("svc", Some("1.0.0")).is_some());
        assert!(cache.get("svc", None).is_some());

        cache.upsert(&detail("1.1.0", Some(false), "draft"));
        assert!(cache.get("svc", Some("1.1.0")).is_some());
        let latest = cache.get("svc", None).expect("latest stays on 1.0.0");
        assert_eq!(
            latest
                .version_detail
                .as_ref()
                .expect("version detail")
                .version,
            "1.0.0"
        );
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
        let canonical = |detail: &McpServerDetailInfo| {
            serde_json::to_string(&serde_json::to_value(detail).expect("detail should serialize"))
                .expect("canonical json should stringify")
        };
        // HashMaps use a random per-instance seed, so repeat the construction to
        // make a missing canonicalization fail reliably rather than probabilistically.
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
            assert_eq!(canonical(&first), canonical(&second));
            let mut cache = McpServerCache::default();
            assert!(cache.upsert(&first));
            assert!(!cache.upsert(&second));
        }
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

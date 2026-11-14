use core::ops::{Deref, DerefMut};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use dashmap::{
    DashMap,
    mapref::one::{Ref, RefMut},
};
use tracing::{Instrument, info, warn};

use crate::api::props::CacheKind;
use crate::common::cache::disk::DiskStore;

use super::executor;

mod disk;

pub(crate) struct Cache<V> {
    inner: Arc<DashMap<String, V>>,
    store: Option<Arc<dyn Store<V>>>,
}

impl<V> Cache<V>
where
    V: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone + 'static,
{
    async fn new(store: Option<Arc<dyn Store<V>>>, load_cache_at_start: bool) -> Self {
        let inner = match &store {
            Some(store) => {
                let map: HashMap<String, V> = if load_cache_at_start {
                    info!("Loading cache by {}", store.name());
                    store.load().await
                } else {
                    info!("Skip loading cache by {}", store.name());
                    HashMap::new()
                };

                let dash_map: DashMap<String, V> = DashMap::with_capacity(map.len());
                for (k, v) in map {
                    dash_map.insert(k, v);
                }
                Arc::new(dash_map)
            }
            None => {
                info!("Creating memory-only cache (no disk store)");
                Arc::new(DashMap::new())
            }
        };

        Self { inner, store }
    }

    pub(crate) fn get(&self, key: &str) -> Option<CacheRef<'_, V>> {
        self.inner
            .get(key)
            .map(|dash_map_ref| CacheRef { dash_map_ref })
    }

    pub(crate) fn get_mut(&self, key: &str) -> Option<CacheRefMut<'_, V>> {
        self.inner.get_mut(key).map(|dash_map_ref_mut| CacheRefMut {
            dash_map_ref_mut,
            store: self.store.clone(),
        })
    }

    pub(crate) fn insert(&self, key: String, value: V) -> Option<V> {
        let ret = self.inner.insert(key.clone(), value);

        if let Some(store) = self.store.clone() {
            let inner = Arc::clone(&self.inner);

            executor::spawn(
                async move {
                    // Hold the shard lock only for the clone; serialization and
                    // disk I/O must not block writers.
                    let snapshot = inner.get(&key).map(|entry| entry.value().clone());
                    let bytes = tokio::task::spawn_blocking(move || {
                        snapshot.as_ref().and_then(|value| serialize_value(value))
                    })
                    .await
                    .ok()
                    .flatten();
                    if let Some(bytes) = bytes {
                        store.save(&key, bytes).await;
                    }
                }
                .in_current_span(),
            );
        }

        ret
    }

    pub(crate) fn remove(&self, key: &str) -> Option<V> {
        let ret = self.inner.remove(key)?;

        if let Some(ref store) = self.store {
            let key_str = ret.0.clone();
            let store = store.clone();

            executor::spawn(
                async move {
                    store.remove(&key_str).await;
                }
                .in_current_span(),
            );
        }

        Some(ret.1)
    }

    /// Remove the persisted copy while keeping the in-memory entry.
    pub(crate) async fn remove_persisted(&self, key: &str) {
        if let Some(store) = &self.store {
            store.remove(key).await;
        }
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub(crate) fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&String, &V),
    {
        for item in self.inner.iter() {
            f(item.key(), item.value());
        }
    }
}

pub(crate) struct CacheRef<'a, V> {
    dash_map_ref: Ref<'a, String, V>,
}

impl<V> Deref for CacheRef<'_, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.dash_map_ref.value()
    }
}

pub(crate) struct CacheRefMut<'a, V>
where
    V: serde::Serialize + Send + Sync + Clone + 'static,
{
    dash_map_ref_mut: RefMut<'a, String, V>,
    store: Option<Arc<dyn Store<V>>>,
}

impl<V> Deref for CacheRefMut<'_, V>
where
    V: serde::Serialize + Send + Sync + Clone + 'static,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.dash_map_ref_mut.value()
    }
}

impl<V> DerefMut for CacheRefMut<'_, V>
where
    V: serde::Serialize + Send + Sync + Clone + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dash_map_ref_mut.value_mut()
    }
}

impl<V> CacheRefMut<'_, V>
where
    V: serde::Serialize + Send + Sync + Clone + 'static,
{
    /// Write the current value to the store in the background.
    ///
    /// Mutations are never persisted implicitly: a caller that wants the disk
    /// cache updated must request it explicitly.
    pub(crate) fn persist(self) {
        let Some(store) = self.store else {
            return;
        };
        let snapshot = self.dash_map_ref_mut.value().clone();
        let key = self.dash_map_ref_mut.key().clone();

        executor::spawn(
            async move {
                let bytes = tokio::task::spawn_blocking(move || serialize_value(&snapshot))
                    .await
                    .ok()
                    .flatten();
                if let Some(bytes) = bytes {
                    store.save(&key, bytes).await;
                }
            }
            .in_current_span(),
        );
    }
}

fn serialize_value<V>(value: &V) -> Option<Vec<u8>>
where
    V: serde::Serialize,
{
    match serde_json::to_vec(value) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            warn!("serialize cache entry failed: {e:?}");
            None
        }
    }
}

pub(crate) struct CacheBuilder<V>
where
    V: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    _mark: PhantomData<V>,
    namespace: String,
    module: String,
    load_cache_at_start: bool,
    store: Option<Arc<dyn Store<V>>>,
}

const CONFIG_MODULE: &str = "config";
const NAMING_MODULE: &str = "naming";

impl<V> CacheBuilder<V>
where
    V: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone + 'static,
{
    pub(crate) fn config(namespace: String) -> Self {
        Self {
            _mark: Default::default(),
            namespace,
            module: CONFIG_MODULE.to_owned(),
            load_cache_at_start: false,
            store: None,
        }
    }

    pub(crate) fn naming(namespace: String) -> Self {
        Self {
            _mark: Default::default(),
            namespace,
            module: NAMING_MODULE.to_owned(),
            load_cache_at_start: false,
            store: None,
        }
    }

    pub(crate) fn load_cache_at_start(self, load_cache_at_start: bool) -> Self {
        Self {
            load_cache_at_start,
            ..self
        }
    }

    pub(crate) fn store(
        self,
        cache_kind: CacheKind,
        cache_dir: Option<std::path::PathBuf>,
    ) -> Self {
        if !cache_kind.is_disk_store() {
            info!(
                cache_kind = cache_kind.name(),
                "Cache store disabled, using memory-only cache"
            );
            return Self {
                store: None,
                ..self
            };
        }

        let mut disk_path = cache_dir.unwrap_or_else(|| {
            std::path::PathBuf::from(crate::common::util::HOME_DIR.to_owned()).join("nacos")
        });
        disk_path.push(self.module.clone());
        disk_path.push(self.namespace.clone());

        let disk_store = Arc::new(DiskStore::new(disk_path));

        Self {
            store: Some(disk_store),
            ..self
        }
    }

    pub(crate) async fn build(self) -> Cache<V> {
        Cache::new(self.store, self.load_cache_at_start).await
    }
}

// Store trait with separate constraints for load and save operations
#[async_trait]
trait Store<V>: Send + Sync {
    fn name(&self) -> &str {
        self.kind().name()
    }

    fn kind(&self) -> CacheKind;

    async fn load(&self) -> HashMap<String, V>
    where
        V: serde::de::DeserializeOwned;

    async fn save(&self, key: &str, value: Vec<u8>);

    async fn remove(&self, key: &str);
}

#[cfg(test)]
pub mod tests {
    use std::time::Duration;

    use crate::{api::props::CacheKind, common::cache::Cache};

    use super::CacheBuilder;

    #[tokio::test]
    async fn test_cache_accepts_str_keys() {
        let cache: Cache<String> = CacheBuilder::config("test-str-keys".to_string())
            .build()
            .await;

        cache.insert("key".to_string(), "value".to_string());
        assert_eq!(
            cache.get("key").as_deref().map(String::as_str),
            Some("value")
        );
        assert!(cache.contains_key("key"));
        assert!(cache.get_mut("key").is_some());
        assert_eq!(cache.remove("key").as_deref(), Some("value"));
        assert!(cache.get("key").is_none());
    }

    #[tokio::test]
    async fn test_write_guard_persists_only_on_request() {
        let root = std::env::temp_dir().join(format!("nacos-sdk-dirty-{}", rand::random::<u64>()));
        let cache: Cache<String> = CacheBuilder::config("test-namespace".to_string())
            .store(CacheKind::DiskStore, Some(root.clone()))
            .build()
            .await;
        let cache_file = root.join("config").join("test-namespace").join("key");

        cache.insert("key".to_string(), "value".to_string());
        crate::test_config::wait_for_file(&cache_file, "initial insert should reach disk").await;

        tokio::fs::remove_file(&cache_file)
            .await
            .expect("cache file should be removable");

        {
            let mut guard = cache.get_mut("key").expect("entry should exist");
            *guard = "modified".to_string();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !cache_file.exists(),
            "dropping a modified write guard must not rewrite the cache file"
        );

        {
            let guard = cache.get_mut("key").expect("entry should exist");
            guard.persist();
        }
        crate::test_config::wait_for_file(&cache_file, "explicit persist should reach disk").await;
        let bytes = tokio::fs::read(&cache_file)
            .await
            .expect("cache file should exist");
        assert_eq!("\"modified\"", String::from_utf8_lossy(&bytes));

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[allow(clippy::disallowed_methods, clippy::disallowed_types)]
    #[tokio::test]
    async fn test_insert_does_not_serialize_on_caller_thread() {
        #[derive(Clone)]
        struct SlowValue;

        impl serde::Serialize for SlowValue {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                std::thread::sleep(Duration::from_millis(400));
                serializer.serialize_unit()
            }
        }

        impl<'de> serde::Deserialize<'de> for SlowValue {
            fn deserialize<D>(_: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Ok(SlowValue)
            }
        }

        let root = std::env::temp_dir().join(format!("nacos-sdk-slow-{}", rand::random::<u64>()));
        let cache: Cache<SlowValue> = CacheBuilder::config("test-namespace".to_string())
            .store(CacheKind::DiskStore, Some(root.clone()))
            .build()
            .await;

        let started = std::time::Instant::now();
        cache.insert("slow".to_string(), SlowValue);
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(150),
            "insert blocked on serialization for {elapsed:?}"
        );

        let cache_file = root.join("config").join("test-namespace").join("slow");
        crate::test_config::wait_for_file(&cache_file, "cache entry should eventually reach disk")
            .await;

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[allow(clippy::disallowed_methods, clippy::disallowed_types)]
    #[tokio::test]
    async fn test_serialization_does_not_hold_the_cache_lock() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        #[derive(Clone)]
        struct BlockingValue(Arc<AtomicBool>);

        impl serde::Serialize for BlockingValue {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.0.store(true, Ordering::Release);
                std::thread::sleep(Duration::from_millis(400));
                serializer.serialize_unit()
            }
        }

        impl<'de> serde::Deserialize<'de> for BlockingValue {
            fn deserialize<D>(_: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Ok(BlockingValue(Arc::new(AtomicBool::new(false))))
            }
        }

        let root = std::env::temp_dir().join(format!("nacos-sdk-lock-{}", rand::random::<u64>()));
        let cache: Cache<BlockingValue> = CacheBuilder::config("test-namespace".to_string())
            .store(CacheKind::DiskStore, Some(root.clone()))
            .build()
            .await;

        let started = Arc::new(AtomicBool::new(false));
        cache.insert("key".to_string(), BlockingValue(Arc::clone(&started)));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("serialization should start");

        let insert_started = std::time::Instant::now();
        cache.insert(
            "key".to_string(),
            BlockingValue(Arc::new(AtomicBool::new(false))),
        );
        let elapsed = insert_started.elapsed();
        assert!(
            elapsed < Duration::from_millis(150),
            "insert blocked on the shard lock for {elapsed:?}"
        );

        tokio::time::sleep(Duration::from_millis(600)).await;
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    pub async fn test_cache() {
        crate::test_config::setup_log();

        let cache: Cache<String> = CacheBuilder::naming("test-naming".to_string())
            .load_cache_at_start(true)
            .store(CacheKind::DiskStore, None)
            .build()
            .await;
        let key = String::from("key");

        {
            let value = cache.get(&key);
            assert!(value.is_none());
        }

        {
            let ret = cache.insert(key.clone(), String::from("value"));
            assert!(ret.is_none());
        }

        {
            let value = cache.get(&key);
            assert!(value.is_some());
            let value = value.expect("Value should be present in cache");
            assert!(value.eq("value"));
        }

        {
            let value = cache.get_mut(&key);
            assert!(value.is_some());
            let mut value = value.expect("Mutable value should be present in cache");
            *value = "modify".to_owned();
        }

        {
            let value = cache.get(&key);
            assert!(value.is_some());
            let value = value.expect("Value should be present in cache after modification");
            assert!(value.eq("modify"));
        }

        {
            tokio::time::sleep(Duration::from_millis(222)).await; // sleep for cache already write into disk
            let ret = cache.remove(&key);
            assert!(ret.is_some());
            let ret = ret.expect("Removed value should be present");
            assert!(ret.eq("modify"));
        }

        {
            let ret = cache.get(&key);
            assert!(ret.is_none());
        }

        {
            let ret = cache.insert("key1".to_string(), "test".to_owned());
            assert!(ret.is_none());
            // sleep 1 second
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        let user_home = std::env::home_dir();

        let mut disk_path =
            user_home.expect("Failed to get user home directory from system environment");
        disk_path.push("nacos");
        disk_path.push("naming");
        disk_path.push("test-naming");
        disk_path.push("key1");

        let read_ret = tokio::fs::read(&disk_path).await;

        assert!(read_ret.is_ok());

        let ret = read_ret.expect("Failed to read cache file from disk");

        let str = String::from_utf8(ret);
        assert!(str.is_ok());

        let str = str.expect("Failed to convert bytes to UTF-8 string");

        assert!(str.eq("\"test\""));

        // drop cache
        drop(cache);

        let cache: Cache<String> = CacheBuilder::naming("test-naming".to_string())
            .load_cache_at_start(true)
            .store(CacheKind::DiskStore, None)
            .build()
            .await;

        let key = String::from("key1");
        let value = cache.get(&key);
        assert!(value.is_some());
        let value = value.expect("Value should be present in cache after reload");
        assert!(value.eq("test"));

        tokio::fs::remove_file(&disk_path)
            .await
            .expect("Failed to remove test cache file");
    }

    #[tokio::test]
    async fn test_custom_cache_dir() {
        let root =
            std::env::temp_dir().join(format!("nacos-sdk-custom-cache-{}", rand::random::<u64>()));
        let cache: Cache<String> = CacheBuilder::config("test-namespace".to_string())
            .store(CacheKind::DiskStore, Some(root.clone()))
            .build()
            .await;

        cache.insert("key".to_string(), "value".to_string());

        let cache_file = root.join("config").join("test-namespace").join("key");
        let value = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(value) = tokio::fs::read_to_string(&cache_file).await {
                    break value;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("custom cache file should be written within timeout");
        assert_eq!(value, "\"value\"");

        tokio::fs::remove_dir_all(root)
            .await
            .expect("custom cache directory should be removable");
    }

    #[tokio::test]
    async fn test_memory_only_cache_skips_disk() {
        let root =
            std::env::temp_dir().join(format!("nacos-sdk-memory-cache-{}", rand::random::<u64>()));

        let cache: Cache<String> = CacheBuilder::config("test-namespace".to_string())
            .load_cache_at_start(true)
            .store(CacheKind::None, Some(root.clone()))
            .build()
            .await;

        assert!(
            !root.exists(),
            "memory-only cache should not create the cache dir"
        );

        cache.insert("key".to_string(), "value".to_string());
        {
            let mut value = cache
                .get_mut("key")
                .expect("Mutable value should be present in cache");
            *value = String::from("modified");
        }
        let removed = cache
            .remove("key")
            .expect("Removed value should be present");
        assert_eq!(removed, "modified");

        // wait for any potential async store write
        tokio::time::sleep(Duration::from_millis(222)).await;

        assert!(cache.get("key").is_none(), "cache should be memory-only");
        assert!(
            !root.exists(),
            "memory-only cache should not write anything to disk, actual: {:?}",
            root.display()
        );

        // nothing persisted, so a rebuilt memory-only cache starts empty
        let cache: Cache<String> = CacheBuilder::config("test-namespace".to_string())
            .load_cache_at_start(true)
            .store(CacheKind::None, Some(root.clone()))
            .build()
            .await;
        assert!(
            cache.get("key").is_none(),
            "memory-only cache should not load anything from disk"
        );
        assert!(!root.exists());

        // cleanup in case of an unexpected leftover
        let _ = tokio::fs::remove_dir_all(&root).await;
    }
}

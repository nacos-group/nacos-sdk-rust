use core::ops::{Deref, DerefMut};
use std::{borrow::Cow, collections::HashMap, marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use dashmap::{
    DashMap,
    mapref::one::{Ref, RefMut},
};
use tracing::{Instrument, info, info_span};

use crate::common::cache::disk::DiskStore;

use super::executor;

mod disk;

pub(crate) struct Cache<V> {
    inner: Arc<DashMap<String, V>>,
    store: Option<Arc<dyn Store<V>>>,
}

impl<V> Cache<V>
where
    V: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    fn new(id: &str, store: Option<Arc<dyn Store<V>>>, load_cache_at_start: bool) -> Self {
        let _span_enter = info_span!("cache", id = id).entered();

        let inner = match &store {
            Some(store) => {
                let map: HashMap<String, V> = if load_cache_at_start {
                    info!("Loading cache by {}", store.name());
                    store.load()
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

    pub(crate) fn get(&self, key: &String) -> Option<CacheRef<'_, V>> {
        self.inner
            .get(key)
            .map(|dash_map_ref| CacheRef { dash_map_ref })
    }

    pub(crate) fn get_mut(&self, key: &String) -> Option<CacheRefMut<'_, V>> {
        self.inner.get_mut(key).map(|dash_map_ref_mut| {
            let key = dash_map_ref_mut.key().clone();
            CacheRefMut {
                dash_map_ref_mut,
                store: self.store.clone(),
                inner: self.inner.clone(),
                key,
            }
        })
    }

    pub(crate) fn insert(&self, key: String, value: V) -> Option<V> {
        if let Some(ref store) = self.store
            && let Ok(bytes) = serde_json::to_vec(&value)
        {
            let store = store.clone();
            let key_str = key.clone();
            executor::spawn(async move { store.save(&key_str, bytes).await }.in_current_span());
        }

        self.inner.insert(key, value)
    }

    #[allow(dead_code)]
    pub(crate) fn remove(&self, key: &String) -> Option<V> {
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

    pub(crate) fn contains_key(&self, key: &String) -> bool {
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
    V: serde::Serialize + Send + Sync + 'static,
{
    dash_map_ref_mut: RefMut<'a, String, V>,
    store: Option<Arc<dyn Store<V>>>,
    inner: Arc<DashMap<String, V>>,
    key: String,
}

impl<V> Deref for CacheRefMut<'_, V>
where
    V: serde::Serialize + Send + Sync + 'static,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.dash_map_ref_mut.value()
    }
}

impl<V> DerefMut for CacheRefMut<'_, V>
where
    V: serde::Serialize + Send + Sync + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dash_map_ref_mut.value_mut()
    }
}

impl<V> Drop for CacheRefMut<'_, V>
where
    V: serde::Serialize + Send + Sync + 'static,
{
    fn drop(&mut self) {
        if let Some(store) = self.store.take() {
            let inner = self.inner.clone();
            let key = self.key.clone();

            executor::spawn(
                async move {
                    let bytes = inner
                        .get(&key)
                        .and_then(|data| serde_json::to_vec(data.value()).ok());
                    if let Some(bytes) = bytes {
                        store.save(&key, bytes).await;
                    }
                }
                .in_current_span(),
            );
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
    V: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
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

    pub(crate) fn disk_store(self) -> Self {
        let user_home =
            home::home_dir().expect("Failed to get user home directory from system environment");

        let mut disk_path = user_home;
        disk_path.push("nacos");
        disk_path.push(self.module.clone());
        disk_path.push(self.namespace.clone());

        let path_buf = disk_path.clone();
        executor::spawn(async { tokio::fs::create_dir_all(path_buf).await });

        let disk_store = Arc::new(DiskStore::new(disk_path));

        Self {
            store: Some(disk_store),
            ..self
        }
    }

    pub(crate) fn build(self, id: String) -> Cache<V> {
        Cache::new(&id, self.store, self.load_cache_at_start)
    }
}

// Store trait with separate constraints for load and save operations
#[async_trait]
trait Store<V>: Send + Sync {
    fn name(&self) -> Cow<'_, str>;

    fn load(&self) -> HashMap<String, V>
    where
        V: serde::de::DeserializeOwned;

    async fn save(&self, key: &str, value: Vec<u8>);

    async fn remove(&self, key: &str);
}

#[cfg(test)]
pub mod tests {
    use std::{thread::sleep, time::Duration};

    use crate::{common::cache::Cache, test_config};

    use super::CacheBuilder;

    fn setup() {
        test_config::setup_log();
    }

    fn teardown() {}

    fn run_test<T, F>(test: F) -> T
    where
        F: FnOnce() -> T,
    {
        setup();
        let ret = test();
        teardown();
        ret
    }

    #[test]
    pub fn test_cache() {
        run_test(|| {
            let cache: Cache<String> = CacheBuilder::naming("test-naming".to_string())
                .load_cache_at_start(true)
                .disk_store()
                .build("test-id".to_string());
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
                sleep(Duration::from_secs(1));
            }

            let user_home = home::home_dir();

            let mut disk_path =
                user_home.expect("Failed to get user home directory from system environment");
            disk_path.push("nacos");
            disk_path.push("naming");
            disk_path.push("test-naming");
            disk_path.push("key1");

            let read_ret = std::fs::read(&disk_path);

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
                .disk_store()
                .build("test-id".to_string());

            let key = String::from("key1");
            let value = cache.get(&key);
            assert!(value.is_some());
            let value = value.expect("Value should be present in cache after reload");
            assert!(value.eq("test"));

            let _ = std::fs::remove_file(&disk_path).expect("Failed to remove test cache file");
        });
    }
}

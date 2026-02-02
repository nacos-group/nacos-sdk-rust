use core::ops::{Deref, DerefMut};
use std::{
    borrow::{Borrow, Cow},
    collections::HashMap,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use dashmap::{
    DashMap,
    mapref::one::{Ref, RefMut},
};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tracing::{Instrument, debug, debug_span, warn};

use crate::common::cache::disk::DiskStore;

use super::executor;

mod disk;

pub(crate) struct Cache<V> {
    inner: Arc<DashMap<VersionKeyWrapper, V>>,
    sender: Option<Sender<ChangeEvent>>,
}

impl<V> Cache<V>
where
    V: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    fn new(id: String, store: Option<Box<dyn Store<V>>>, load_cache_at_start: bool) -> Self {
        let _span_enter = debug_span!("cache", id = id).entered();

        let (dash_map, sender) = if let Some(mut store) = store {
            let map = if load_cache_at_start {
                store.load()
            } else {
                HashMap::new()
            };

            let dash_map: DashMap<VersionKeyWrapper, V> = DashMap::with_capacity(map.len());
            let dash_map = Arc::new(dash_map);
            for (k, v) in map {
                dash_map.insert(VersionKeyWrapper::new(k), v);
            }

            let (sender, receiver) = channel::<ChangeEvent>(1024);
            executor::spawn(Cache::sync_data(dash_map.clone(), receiver, store).in_current_span());

            (dash_map, Some(sender))
        } else {
            (Arc::new(DashMap::new()), None)
        };

        Self {
            inner: dash_map,
            sender,
        }
    }

    async fn sync_data(
        cache: Arc<DashMap<VersionKeyWrapper, V>>,
        mut receiver: Receiver<ChangeEvent>,
        mut store: Box<dyn Store<V>>,
    ) {
        debug!("sync to {} started!", store.name());

        while let Some(event) = receiver.recv().await {
            match event {
                ChangeEvent::Insert(current_version, key)
                | ChangeEvent::Modify(current_version, key) => {
                    let refresh_ret = key.sync(current_version);
                    if !refresh_ret {
                        continue;
                    }

                    let value = {
                        let data = cache.get(&key);
                        if let Some(data) = data {
                            let value = data.value();
                            let value = serde_json::ser::to_vec(value);
                            if let Err(e) = value {
                                warn!("cache data cannot serialize to bytes. {}", e);
                                continue;
                            }

                            let value = value.expect("Failed to serialize cache data to bytes");
                            Some(value)
                        } else {
                            None
                        }
                    };
                    if let Some(value) = value {
                        store.save(key.as_str(), value).await;
                    }
                }
                ChangeEvent::Remove(current_version, key) => {
                    let refresh_ret = key.sync(current_version);
                    if !refresh_ret {
                        continue;
                    }
                    store.remove(key.as_str()).await;
                }
            }
        }
        debug!("sync to {} quit!", store.name());
    }

    pub(crate) fn get(&self, key: &String) -> Option<CacheRef<'_, V>> {
        let value = self.inner.get(key);
        value.map(|dash_map_ref| CacheRef { dash_map_ref })
    }

    pub(crate) fn get_mut(&self, key: &String) -> Option<CacheRefMut<'_, V>> {
        let value = self.inner.get_mut(key);
        value.map(|dash_map_ref_mut| CacheRefMut {
            dash_map_ref_mut,
            sender: self.sender.clone(),
        })
    }

    pub(crate) fn insert(&self, key: String, value: V) -> Option<V> {
        let key = VersionKeyWrapper::new(key);
        let ret = self.inner.insert(key.clone(), value);

        if let Some(ref sender) = self.sender {
            let insert_event = ChangeEvent::Insert(key.refresh(), key);
            let sender = sender.clone();
            executor::spawn(async move { sender.send(insert_event).await });
        }

        ret
    }

    pub(crate) fn remove(&self, key: &String) -> Option<V> {
        let ret = self.inner.remove(key);
        match ret {
            None => None,
            Some((key, value)) => {
                if let Some(ref sender) = self.sender {
                    let remove_event = ChangeEvent::Remove(key.refresh(), key);
                    let sender = sender.clone();
                    executor::spawn(async move { sender.send(remove_event).await });
                }

                Some(value)
            }
        }
    }

    pub(crate) fn contains_key(&self, key: &String) -> bool {
        self.inner.contains_key(key)
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct VersionKeyWrapper(Arc<VersionKey>);

impl Clone for VersionKeyWrapper {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl VersionKeyWrapper {
    fn new(key: String) -> Self {
        Self(Arc::new(VersionKey::new(key)))
    }

    fn refresh(&self) -> usize {
        self.0.refresh()
    }

    fn sync(&self, version: usize) -> bool {
        self.0.sync(version)
    }
}

impl Borrow<String> for VersionKeyWrapper {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl Deref for VersionKeyWrapper {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
struct VersionKey {
    raw_key: String,
    version: AtomicUsize,
    sync_version: AtomicUsize,
}

impl std::hash::Hash for VersionKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.raw_key.hash(state);
    }
}

impl PartialEq for VersionKey {
    fn eq(&self, other: &Self) -> bool {
        self.raw_key == other.raw_key
    }
}

impl std::cmp::Eq for VersionKey {}

impl VersionKey {
    fn new(key: String) -> Self {
        Self {
            raw_key: key,
            version: AtomicUsize::new(1),
            sync_version: AtomicUsize::new(1),
        }
    }

    fn refresh(&self) -> usize {
        self.version.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn sync(&self, version: usize) -> bool {
        loop {
            let sync_version = self.sync_version.load(Ordering::Acquire);
            if version > sync_version {
                let ret = self.sync_version.compare_exchange(
                    sync_version,
                    version,
                    Ordering::SeqCst,
                    Ordering::Acquire,
                );
                if ret.is_ok() {
                    return true;
                } else {
                    continue;
                }
            } else {
                return false;
            }
        }
    }
}

impl Deref for VersionKey {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.raw_key
    }
}

pub(crate) struct CacheRef<'a, V> {
    dash_map_ref: Ref<'a, VersionKeyWrapper, V>,
}

impl<V> Deref for CacheRef<'_, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.dash_map_ref.value()
    }
}

pub(crate) struct CacheRefMut<'a, V> {
    dash_map_ref_mut: RefMut<'a, VersionKeyWrapper, V>,
    sender: Option<Sender<ChangeEvent>>,
}

impl<V> Deref for CacheRefMut<'_, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.dash_map_ref_mut.value()
    }
}

impl<V> DerefMut for CacheRefMut<'_, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dash_map_ref_mut.value_mut()
    }
}

impl<V> Drop for CacheRefMut<'_, V> {
    fn drop(&mut self) {
        let key = self.dash_map_ref_mut.key().clone();

        if let Some(ref sender) = self.sender {
            let modify_event = ChangeEvent::Modify(key.refresh(), key);
            let sender = sender.clone();
            executor::spawn(async move { sender.send(modify_event).await });
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
    store: Option<Box<dyn Store<V>>>,
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
        // get user home directory
        let user_home = home::home_dir();
        if user_home.is_none() {
            panic!("cannot read user home variable from system environment.")
        }

        let mut disk_path =
            user_home.expect("Failed to get user home directory from system environment");
        disk_path.push("nacos");
        disk_path.push(self.module.clone());
        disk_path.push(self.namespace.clone());

        let disk_store = Box::new(DiskStore::new(disk_path)) as Box<dyn Store<V>>;

        Self {
            store: Some(disk_store),
            ..self
        }
    }

    pub(crate) fn build(self, id: String) -> Cache<V> {
        Cache::new(id, self.store, self.load_cache_at_start)
    }
}

enum ChangeEvent {
    Insert(usize, VersionKeyWrapper),
    Remove(usize, VersionKeyWrapper),
    Modify(usize, VersionKeyWrapper),
}

#[async_trait]
trait Store<V>: Send {
    fn name(&self) -> Cow<'_, str>;

    fn load(&mut self) -> HashMap<String, V>;

    async fn save(&mut self, key: &str, value: Vec<u8>);

    async fn remove(&mut self, key: &str);
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

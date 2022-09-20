use crate::api::client_config::ClientConfig;
use crate::api::config::ConfigResponse;
use crate::config::util;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct ConfigWorker {
    client_config: ClientConfig,
    cache_data_map: Arc<Mutex<HashMap<String, CacheData>>>,
}

impl ConfigWorker {
    pub(crate) fn new(client_config: ClientConfig) -> Self {
        let client_worker = Self {
            client_config,
            cache_data_map: Arc::new(Mutex::new(HashMap::new())),
        };

        tokio::spawn(Self::list_ensure_cache_data_newest());

        client_worker
    }

    /// List-Watch, list ensure cache-data newest.
    async fn list_ensure_cache_data_newest() {
        loop {
            // todo query from server
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }

    /// Add listener.
    pub(crate) fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        tenant: String,
        listener: Box<crate::api::config::ConfigChangeListener>,
    ) {
        let group_key = util::group_key(&data_id, &group, &tenant);
        loop {
            let cache_lock = self.cache_data_map.try_lock();
            if let Ok(mut mutex) = cache_lock {
                if !mutex.contains_key(group_key.as_str()) {
                    mutex.insert(
                        group_key.clone(),
                        CacheData::new(data_id.clone(), group.clone(), tenant.clone()),
                    );
                }
                let _ = mutex
                    .get_mut(group_key.as_str())
                    .map(|c| c.add_listener(listener));
                break;
            }
        }
    }

    /// notify config change
    pub(crate) fn notify_config_change(&mut self, data_id: String, group: String, tenant: String) {
        let group_key = util::group_key(&data_id, &group, &tenant);
        loop {
            let cache_lock = self.cache_data_map.try_lock();
            if let Ok(mut mutex) = cache_lock {
                if !mutex.contains_key(group_key.as_str()) {
                    break;
                }
                let _ = mutex.get_mut(group_key.as_str()).map(|c| {
                    // todo get the newest config to notify
                    c.notify_listener(ConfigResponse::new(
                        c.data_id.clone(),
                        c.group.clone(),
                        c.tenant.clone(),
                        c.content.clone(),
                        c.content_type.clone(),
                    ))
                });
                break;
            }
        }
    }
}

/// Cache Data for Config
#[derive(Clone, Default)]
struct CacheData {
    data_id: String,
    group: String,
    tenant: String,
    /// Default text; text, json, properties, html, xml, yaml ...
    content_type: String,
    content: String,
    md5: String,
    /// whether content was encrypted with encryptedDataKey.
    encrypted_data_key: Option<String>,
    last_modified: i64,

    /// There are some logical differences in the initialization phase, such as no notification of config changed
    initializing: bool,
    /// Mark the cache config is not the latest, need to query the server for synchronize
    need_sync_server: bool,

    /// who listen of config change.
    listeners: Arc<Mutex<Vec<Box<crate::api::config::ConfigChangeListener>>>>,
}

impl CacheData {
    fn new(data_id: String, group: String, tenant: String) -> Self {
        Self {
            data_id,
            group,
            tenant,
            content_type: "text".to_string(),
            ..Default::default()
        }
    }

    /// Add listener.
    fn add_listener(&mut self, listener: Box<crate::api::config::ConfigChangeListener>) {
        loop {
            let listen_lock = self.listeners.try_lock();
            if let Ok(mut mutex) = listen_lock {
                mutex.push(listener);
                break;
            }
        }
    }

    /// Notify listener.
    fn notify_listener(&mut self, config_response: ConfigResponse) {
        loop {
            let listen_lock = self.listeners.try_lock();
            if let Ok(mut mutex) = listen_lock {
                for listen in mutex.iter_mut() {
                    (listen)(config_response.clone());
                }
                break;
            }
        }
    }
}

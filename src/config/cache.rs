use crate::api::config::ConfigResponse;
use crate::api::plugin::ConfigFilter;
use crate::api::plugin::ConfigResp;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

/// Cache Data for Config
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct CacheData {
    pub data_id: String,
    pub group: String,
    pub namespace: String,
    /// Default text; text, json, properties, html, xml, yaml ...
    pub content_type: String,
    pub content: String,
    pub md5: String,
    /// whether content was encrypted with encryptedDataKey.
    pub encrypted_data_key: String,
    pub last_modified: i64,

    /// There are some logical differences in the initialization phase, such as no notification of config changed
    #[serde(skip)]
    pub initializing: bool,

    /// who listen of config change. (runtime only, don't persist)
    #[serde(skip)]
    pub listeners: Arc<Mutex<Vec<ListenerWrapper>>>,

    #[serde(skip)]
    pub config_filters: Arc<Vec<Box<dyn ConfigFilter>>>,
}

impl CacheData {
    pub fn new(
        config_filters: Arc<Vec<Box<dyn ConfigFilter>>>,
        data_id: String,
        group: String,
        namespace: String,
    ) -> Self {
        Self {
            config_filters,
            data_id,
            group,
            namespace,
            content_type: "text".to_string(),
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
    pub async fn notify_listener(&mut self) {
        tracing::info!(
            "notify_listener, dataId={},group={},namespace={},md5={}",
            self.data_id,
            self.group,
            self.namespace,
            self.md5
        );

        let config_resp = self.get_config_resp_after_filter().await;

        if let Ok(mut mutex) = self.listeners.lock() {
            for listen_wrap in mutex.iter_mut() {
                if listen_wrap.last_md5.eq(&self.md5) {
                    continue;
                }
                // Notify when last-md5 not equals the-newest-md5, Notify in independent thread.
                let l_clone = listen_wrap.listener.clone();
                let c_clone = config_resp.clone();
                crate::common::executor::spawn(async move {
                    l_clone.notify(c_clone);
                });
                listen_wrap.last_md5 = self.md5.clone();
            }
        }
    }

    /// Get config response after applying config_filters
    pub(crate) async fn get_config_resp_after_filter(&self) -> ConfigResponse {
        let mut conf_resp = ConfigResp::new(
            self.data_id.clone(),
            self.group.clone(),
            self.namespace.clone(),
            self.content.clone(),
            self.encrypted_data_key.clone(),
        );
        for config_filter in self.config_filters.iter() {
            config_filter.filter(None, Some(&mut conf_resp)).await;
        }

        ConfigResponse::new(
            conf_resp.data_id,
            conf_resp.group,
            conf_resp.namespace,
            conf_resp.content,
            self.content_type.clone(),
            self.md5.clone(),
        )
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

/// The inner Wrapper of ConfigChangeListener
pub(crate) struct ListenerWrapper {
    /// last md5 be notified
    last_md5: String,
    listener: Arc<dyn crate::api::config::ConfigChangeListener>,
}

impl ListenerWrapper {
    fn new(listener: Arc<dyn crate::api::config::ConfigChangeListener>) -> Self {
        Self {
            last_md5: "".to_string(),
            listener,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use crate::config::cache::CacheData;
    use std::sync::Arc;

    #[test]
    fn test_cache_data_add_listener() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let mut cache_data = CacheData::new(Arc::new(Vec::new()), d, g, n);

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

        let mut cache_data = CacheData::new(Arc::new(Vec::new()), d, g, n);

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
}

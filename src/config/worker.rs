use crate::api::config::ConfigResponse;
use crate::api::plugin::{AuthContext, AuthPlugin, ConfigFilter, ConfigReq, ConfigResp};
use crate::api::props::ClientProps;
use crate::common::remote::grpc::message::{GrpcRequestMessage, GrpcResponseMessage};
use crate::common::remote::grpc::{NacosGrpcClient, NacosGrpcClientBuilder};
use crate::config::cache::CacheData;
use crate::config::handler::ConfigChangeNotifyHandler;
use crate::config::message::request::*;
use crate::config::message::response::*;
use crate::config::util;
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug_span, instrument, Instrument};

#[derive(Clone)]
pub(crate) struct ConfigWorker {
    client_props: ClientProps,
    remote_client: Arc<NacosGrpcClient>,
    auth_plugin: Arc<dyn AuthPlugin>,
    cache_data_map: Arc<Mutex<HashMap<String, CacheData>>>,
    config_filters: Arc<Vec<Box<dyn ConfigFilter>>>,
    client_id: String,
}

impl ConfigWorker {
    pub(crate) fn new(
        client_props: ClientProps,
        auth_plugin: Arc<dyn AuthPlugin>,
        config_filters: Vec<Box<dyn ConfigFilter>>,
        client_id: String,
    ) -> crate::api::error::Result<Self> {
        let _config_init = debug_span!("config_init", client_id = client_id).entered();

        let cache_data_map = Arc::new(Mutex::new(HashMap::new()));
        let config_filters = Arc::new(config_filters);

        // group_key: String
        let (notify_change_tx, notify_change_rx) = tokio::sync::mpsc::channel(16);
        let notify_change_tx_clone = notify_change_tx.clone();

        let remote_client = NacosGrpcClientBuilder::new(client_id.clone())
            .address(client_props.server_addr.clone())
            .grpc_port(client_props.grpc_port)
            .namespace(client_props.namespace.clone())
            .app_name(client_props.app_name.clone())
            .client_version(client_props.client_version.clone())
            .support_remote_connection(true)
            .support_config_remote_metrics(true)
            .support_naming_delta_push(false)
            .support_naming_remote_metric(false)
            .add_label(
                crate::api::constants::common_remote::LABEL_SOURCE.to_owned(),
                crate::api::constants::common_remote::LABEL_SOURCE_SDK.to_owned(),
            )
            .add_label(
                crate::api::constants::common_remote::LABEL_MODULE.to_owned(),
                crate::api::constants::common_remote::LABEL_MODULE_CONFIG.to_owned(),
            )
            .add_labels(client_props.labels.clone())
            .register_bi_call_handler::<ConfigChangeNotifyRequest>(Arc::new(
                ConfigChangeNotifyHandler {
                    notify_change_tx,
                    client_id: client_id.clone(),
                },
            ))
            .build()?;

        // todo Event/Subscriber instead of mpsc Sender/Receiver
        crate::common::executor::spawn(
            Self::notify_change_to_cache_data(
                Arc::clone(&remote_client),
                Arc::clone(&auth_plugin),
                Arc::clone(&cache_data_map),
                notify_change_rx,
            )
            .instrument(debug_span!(
                parent: None,
                "notify_change_to_cache_data",
                client_id = client_id
            )),
        );

        crate::common::executor::spawn(
            Self::list_ensure_cache_data_newest(
                Arc::clone(&remote_client),
                Arc::clone(&auth_plugin),
                Arc::clone(&cache_data_map),
                notify_change_tx_clone,
            )
            .instrument(debug_span!(
                parent: None,
                "list_ensure_cache_data_newest",
                client_id = client_id
            )),
        );

        let server_list = Arc::new(client_props.get_server_list()?);
        let plugin = Arc::clone(&auth_plugin);
        let auth_context =
            Arc::new(AuthContext::default().add_params(client_props.auth_context.clone()));
        plugin.set_server_list(server_list.to_vec());
        plugin.login((*auth_context).clone());

        debug_span!("config_auth_task", client_id = client_id).in_scope(|| {
            crate::common::executor::schedule_at_fixed_delay(
                move || {
                    plugin.set_server_list(server_list.to_vec());
                    plugin.login((*auth_context).clone());
                    tracing::debug!("auth_plugin schedule at fixed delay");
                    Ok(())
                },
                tokio::time::Duration::from_secs(30),
            );
        });

        Ok(Self {
            client_props,
            remote_client,
            auth_plugin,
            cache_data_map,
            config_filters,
            client_id,
        })
    }
}

impl ConfigWorker {
    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<ConfigResponse> {
        let namespace = self.client_props.namespace.clone();
        let config_resp = Self::get_config_inner_async(
            self.remote_client.clone(),
            self.auth_plugin.clone(),
            data_id.clone(),
            group.clone(),
            namespace.clone(),
        )
        .await?;

        // invoke config_filter
        let mut conf_resp = ConfigResp::new(
            data_id,
            group,
            namespace,
            config_resp.content.unwrap(),
            config_resp.encrypted_data_key.unwrap_or_default(),
        );
        for config_filter in self.config_filters.iter() {
            config_filter.filter(None, Some(&mut conf_resp));
        }

        Ok(ConfigResponse::new(
            conf_resp.data_id,
            conf_resp.group,
            conf_resp.namespace,
            conf_resp.content,
            config_resp.content_type.unwrap(),
            config_resp.md5.unwrap(),
        ))
    }

    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn publish_config(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> crate::api::error::Result<bool> {
        let namespace = self.client_props.namespace.clone();

        let mut conf_req = ConfigReq::new(data_id, group, namespace, content, "".to_string());
        for config_filter in self.config_filters.iter() {
            config_filter.filter(Some(&mut conf_req), None);
        }

        Self::publish_config_inner_async(
            self.remote_client.clone(),
            self.auth_plugin.clone(),
            conf_req.data_id,
            conf_req.group,
            conf_req.namespace,
            conf_req.content,
            content_type,
            conf_req.encrypted_data_key,
            None,
            None,
            None,
        )
        .await
    }

    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn publish_config_cas(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> crate::api::error::Result<bool> {
        let namespace = self.client_props.namespace.clone();

        let mut conf_req = ConfigReq::new(data_id, group, namespace, content, "".to_string());
        for config_filter in self.config_filters.iter() {
            config_filter.filter(Some(&mut conf_req), None);
        }

        Self::publish_config_inner_async(
            self.remote_client.clone(),
            self.auth_plugin.clone(),
            conf_req.data_id,
            conf_req.group,
            conf_req.namespace,
            conf_req.content,
            content_type,
            conf_req.encrypted_data_key,
            Some(cas_md5),
            None,
            None,
        )
        .await
    }

    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn publish_config_beta(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> crate::api::error::Result<bool> {
        let namespace = self.client_props.namespace.clone();

        let mut conf_req = ConfigReq::new(data_id, group, namespace, content, "".to_string());
        for config_filter in self.config_filters.iter() {
            config_filter.filter(Some(&mut conf_req), None);
        }

        Self::publish_config_inner_async(
            self.remote_client.clone(),
            self.auth_plugin.clone(),
            conf_req.data_id,
            conf_req.group,
            conf_req.namespace,
            conf_req.content,
            content_type,
            conf_req.encrypted_data_key,
            None,
            Some(beta_ips),
            None,
        )
        .await
    }

    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn publish_config_param(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: HashMap<String, String>,
    ) -> crate::api::error::Result<bool> {
        let namespace = self.client_props.namespace.clone();

        let mut conf_req = ConfigReq::new(data_id, group, namespace, content, "".to_string());
        for config_filter in self.config_filters.iter() {
            config_filter.filter(Some(&mut conf_req), None);
        }

        Self::publish_config_inner_async(
            self.remote_client.clone(),
            self.auth_plugin.clone(),
            conf_req.data_id,
            conf_req.group,
            conf_req.namespace,
            conf_req.content,
            content_type,
            conf_req.encrypted_data_key,
            cas_md5,
            None,
            Some(params),
        )
        .await
    }

    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn remove_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<bool> {
        let namespace = self.client_props.namespace.clone();
        Self::remove_config_inner_async(
            self.remote_client.clone(),
            self.auth_plugin.clone(),
            data_id,
            group,
            namespace,
        )
        .await
    }

    /// Add listener.
    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn add_listener(
        &self,
        data_id: String,
        group: String,
        listener: Arc<dyn crate::api::config::ConfigChangeListener>,
    ) {
        let namespace = self.client_props.namespace.clone();
        let group_key = util::group_key(&data_id, &group, &namespace);

        let mut mutex = self.cache_data_map.lock().await;
        if !mutex.contains_key(group_key.as_str()) {
            let mut cache_data =
                CacheData::new(self.config_filters.clone(), data_id, group, namespace);

            // listen immediately upon initialization
            let config_resp = Self::get_config_inner_async(
                self.remote_client.clone(),
                self.auth_plugin.clone(),
                cache_data.data_id.clone(),
                cache_data.group.clone(),
                cache_data.namespace.clone(),
            )
            .in_current_span()
            .await;
            if let Ok(config_resp) = config_resp {
                Self::fill_data_and_notify(&mut cache_data, config_resp);
            }
            let req = ConfigBatchListenRequest::new(true).add_config_listen_context(
                ConfigListenContext::new(
                    cache_data.data_id.clone(),
                    cache_data.group.clone(),
                    cache_data.namespace.clone(),
                    cache_data.md5.clone(),
                ),
            );
            let remote_client_clone = self.remote_client.clone();
            crate::common::executor::spawn(
                async move {
                    let _ = remote_client_clone
                    .unary_call_async::<ConfigBatchListenRequest, ConfigChangeBatchListenResponse>(
                        req,
                    )
                    .await;
                }
                .in_current_span(),
            );

            mutex.insert(group_key.clone(), cache_data);
        }
        let _ = mutex
            .get_mut(group_key.as_str())
            .map(|c| c.add_listener(listener));
    }

    /// Remove listener.
    #[instrument( fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn remove_listener(
        &self,
        data_id: String,
        group: String,
        listener: Arc<dyn crate::api::config::ConfigChangeListener>,
    ) {
        let namespace = self.client_props.namespace.clone();
        let group_key = util::group_key(&data_id, &group, &namespace);

        let mut mutex = self.cache_data_map.lock().await;
        if !mutex.contains_key(group_key.as_str()) {
            return;
        }
        let _ = mutex
            .get_mut(group_key.as_str())
            .map(|c| c.remove_listener(listener));
    }
}

impl ConfigWorker {
    /// List-Watch, list ensure cache-data newest.
    async fn list_ensure_cache_data_newest(
        remote_client: Arc<NacosGrpcClient>,
        auth_plugin: Arc<dyn AuthPlugin>,
        cache_data_map: Arc<Mutex<HashMap<String, CacheData>>>,
        notify_change_tx: tokio::sync::mpsc::Sender<String>,
    ) {
        loop {
            // todo invoke remove_listener with ConfigBatchListenClientRequest::new(false) when is_empty(),
            //  and then remove cache_data from cache_data_map.
            let mut listen_context_vec = Vec::with_capacity(6);
            {
                // try_lock, The failure to acquire the lock can be handled by the next loop.
                if let Ok(mutex) = cache_data_map.try_lock() {
                    for c in mutex.values() {
                        listen_context_vec.push(ConfigListenContext::new(
                            c.data_id.clone(),
                            c.group.clone(),
                            c.namespace.clone(),
                            c.md5.clone(),
                        ));
                    }
                }
            }
            if !listen_context_vec.is_empty() {
                let mut req =
                    ConfigBatchListenRequest::new(true).config_listen_context(listen_context_vec);
                req.add_headers(auth_plugin.get_login_identity().contexts);

                let resp = remote_client
                    .unary_call_async::<ConfigBatchListenRequest, ConfigChangeBatchListenResponse>(
                        req,
                    )
                    .in_current_span()
                    .await;

                if let Ok(resp) = resp {
                    if resp.is_success() {
                        if let Some(change_context_vec) = resp.changed_configs {
                            for context in change_context_vec {
                                // notify config change
                                let group_key = util::group_key(
                                    &context.data_id,
                                    &context.group,
                                    &context.namespace,
                                );
                                let _ = notify_change_tx.send(group_key).await;
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }

    fn fill_data_and_notify(cache_data: &mut CacheData, config_resp: ConfigQueryResponse) {
        cache_data.content_type = config_resp.content_type.unwrap();
        cache_data.content = config_resp.content.unwrap();
        cache_data.md5 = config_resp.md5.unwrap();
        // Compatibility None < 2.1.0
        cache_data.encrypted_data_key = config_resp.encrypted_data_key.unwrap_or_default();
        cache_data.last_modified = config_resp.last_modified;
        tracing::info!("fill_data_and_notify, cache_data={}", cache_data);
        if cache_data.initializing {
            cache_data.initializing = false;
        } else {
            // check md5 and then notify
            cache_data.notify_listener();
        }
    }
}

impl ConfigWorker {
    /// Notify change to cache_data.
    async fn notify_change_to_cache_data(
        remote_client: Arc<NacosGrpcClient>,
        auth_plugin: Arc<dyn AuthPlugin>,
        cache_data_map: Arc<Mutex<HashMap<String, CacheData>>>,
        mut notify_change_rx: tokio::sync::mpsc::Receiver<String>,
    ) {
        loop {
            match notify_change_rx.recv().await {
                None => {
                    tracing::warn!(
                        "notify_change_to_cache_data break, notify_change_rx be dropped(shutdown).",
                    );
                    break;
                }
                Some(group_key) => {
                    let mut mutex = cache_data_map.lock().await;

                    if !mutex.contains_key(group_key.as_str()) {
                        continue;
                    }
                    if let Some(data) = mutex.get_mut(group_key.as_str()) {
                        // get the newest config to notify
                        let config_resp = Self::get_config_inner_async(
                            remote_client.clone(),
                            auth_plugin.clone(),
                            // auth_plugin.clone(),
                            data.data_id.clone(),
                            data.group.clone(),
                            data.namespace.clone(),
                        )
                        .in_current_span()
                        .await;
                        if let Ok(config_resp) = config_resp {
                            Self::fill_data_and_notify(data, config_resp);
                        }
                    }
                }
            }
        }
    }
}

impl ConfigWorker {
    async fn get_config_inner_async(
        remote_client: Arc<NacosGrpcClient>,
        auth_plugin: Arc<dyn AuthPlugin>,
        data_id: String,
        group: String,
        namespace: String,
    ) -> crate::api::error::Result<ConfigQueryResponse> {
        let mut req = ConfigQueryRequest::new(data_id, group, namespace);
        req.add_headers(auth_plugin.get_login_identity().contexts);
        let resp = remote_client
            .unary_call_async::<ConfigQueryRequest, ConfigQueryResponse>(req)
            .await?;

        if resp.is_success() {
            Ok(resp)
        } else if resp.is_not_found() {
            Err(crate::api::error::Error::ConfigNotFound(format!(
                "error_code={},message={}",
                resp.error_code,
                resp.message.unwrap()
            )))
        } else if resp.is_query_conflict() {
            Err(crate::api::error::Error::ConfigQueryConflict(format!(
                "error_code={},message={}",
                resp.error_code,
                resp.message.unwrap()
            )))
        } else {
            let ConfigQueryResponse {
                error_code,
                message,
                ..
            } = resp;
            Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                error_code,
                message.unwrap_or_default(),
            )))
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn publish_config_inner_async(
        remote_client: Arc<NacosGrpcClient>,
        auth_plugin: Arc<dyn AuthPlugin>,
        data_id: String,
        group: String,
        namespace: String,
        content: String,
        content_type: Option<String>,
        encrypted_data_key: String,
        cas_md5: Option<String>,
        beta_ips: Option<String>,
        params: Option<HashMap<String, String>>,
    ) -> crate::api::error::Result<bool> {
        let mut req =
            ConfigPublishRequest::new(data_id, group, namespace, content).cas_md5(cas_md5);
        req.add_headers(auth_plugin.get_login_identity().contexts);

        // Customize parameters have low priority
        if let Some(params) = params {
            req.add_addition_params(params);
        }
        if let Some(content_type) = content_type {
            req.add_addition_param(
                crate::api::config::constants::KEY_PARAM_CONTENT_TYPE,
                content_type,
            );
        }
        if let Some(beta_ips) = beta_ips {
            req.add_addition_param(crate::api::config::constants::KEY_PARAM_BETA_IPS, beta_ips);
        }
        req.add_addition_param(
            crate::api::config::constants::KEY_PARAM_ENCRYPTED_DATA_KEY,
            encrypted_data_key,
        );
        let resp = remote_client
            .unary_call_async::<ConfigPublishRequest, ConfigPublishResponse>(req)
            .await?;

        if resp.is_success() {
            Ok(true)
        } else {
            let ConfigPublishResponse {
                error_code,
                message,
                ..
            } = resp;
            Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                error_code,
                message.unwrap_or_default(),
            )))
        }
    }

    async fn remove_config_inner_async(
        remote_client: Arc<NacosGrpcClient>,
        auth_plugin: Arc<dyn AuthPlugin>,
        data_id: String,
        group: String,
        namespace: String,
    ) -> crate::api::error::Result<bool> {
        let mut req = ConfigRemoveRequest::new(data_id, group, namespace);
        req.add_headers(auth_plugin.get_login_identity().contexts);
        let resp = remote_client
            .unary_call_async::<ConfigRemoveRequest, ConfigRemoveResponse>(req)
            .await?;

        if resp.is_success() {
            Ok(true)
        } else {
            let ConfigRemoveResponse {
                error_code,
                message,
                ..
            } = resp;
            Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                error_code,
                message.unwrap_or_default(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use crate::api::plugin::NoopAuthPlugin;
    use crate::api::props::ClientProps;
    use crate::config::util;
    use crate::config::worker::ConfigWorker;
    use std::sync::Arc;

    #[tokio::test]
    #[ignore]
    async fn test_client_worker_add_listener() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let client_worker = ConfigWorker::new(
            ClientProps::new().namespace(n.clone()),
            Arc::new(NoopAuthPlugin::default()),
            Vec::new(),
            "test_config".to_string(),
        )
        .unwrap();

        // test add listener1
        let lis1_arc = Arc::new(TestConfigChangeListener1 {});
        let _listen = client_worker.add_listener(d.clone(), g.clone(), lis1_arc);

        // test add listener2
        let lis2_arc = Arc::new(TestConfigChangeListener2 {});
        let _listen = client_worker.add_listener(d.clone(), g.clone(), lis2_arc.clone());
        // test add a listener2 again
        let _listen = client_worker.add_listener(d.clone(), g.clone(), lis2_arc);

        let group_key = util::group_key(&d, &g, &n);
        {
            let cache_data_map_mutex = client_worker.cache_data_map.lock().await;
            let cache_data = cache_data_map_mutex.get(group_key.as_str()).unwrap();
            let listen_mutex = cache_data.listeners.lock().unwrap();
            assert_eq!(2, listen_mutex.len());
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_client_worker_add_listener_then_remove() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let client_worker = ConfigWorker::new(
            ClientProps::new().namespace(n.clone()),
            Arc::new(NoopAuthPlugin::default()),
            Vec::new(),
            "test_config".to_string(),
        )
        .unwrap();

        // test add listener1
        let lis1_arc = Arc::new(TestConfigChangeListener1 {});
        let lis1_arc2 = Arc::clone(&lis1_arc);
        let _listen = client_worker.add_listener(d.clone(), g.clone(), lis1_arc);

        let group_key = util::group_key(&d, &g, &n);
        {
            let cache_data_map_mutex = client_worker.cache_data_map.lock().await;
            let cache_data = cache_data_map_mutex.get(group_key.as_str()).unwrap();
            let listen_mutex = cache_data.listeners.lock().unwrap();
            assert_eq!(1, listen_mutex.len());
        }

        client_worker
            .remove_listener(d.clone(), g.clone(), lis1_arc2)
            .await;
        {
            let cache_data_map_mutex = client_worker.cache_data_map.lock().await;
            let cache_data = cache_data_map_mutex.get(group_key.as_str()).unwrap();
            let listen_mutex = cache_data.listeners.lock().unwrap();
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

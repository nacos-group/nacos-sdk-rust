use crate::api::config::ConfigResponse;
use crate::api::plugin::{AuthPlugin, ConfigFilter, ConfigReq, ConfigResp};
use crate::api::props::ClientProps;
use crate::common::cache::{Cache, CacheBuilder};
use crate::common::remote::grpc::message::GrpcResponseMessage;
use crate::common::remote::grpc::{NacosGrpcClient, NacosGrpcClientBuilder};
use crate::common::remote::server_list::create_server_list_provider;
use crate::config::cache::CacheData;
use crate::config::handler::ConfigChangeNotifyHandler;
use crate::config::message::request::*;
use crate::config::message::response::*;
use crate::config::util;
use std::collections::HashMap;
use std::sync::Arc;

use tracing::{Instrument, instrument};

#[derive(Clone)]
pub(crate) struct ConfigWorker {
    namespace: String,
    remote_client: Arc<NacosGrpcClient>,
    unified_cache: Arc<Cache<CacheData>>,
    config_filters: Arc<Vec<Box<dyn ConfigFilter>>>,
}

impl ConfigWorker {
    pub(crate) async fn new(
        client_props: ClientProps,
        auth_plugin: Arc<dyn AuthPlugin>,
        config_filters: Vec<Box<dyn ConfigFilter>>,
        client_id: String,
    ) -> crate::api::error::Result<Self> {
        let namespace = client_props.get_namespace();
        let cache_ns = client_props.get_namespace_default_if_empty();
        let cache_kind = client_props.get_cache_kind();
        let config_load_cache_at_start = client_props.get_config_load_cache_at_start();
        if !cache_kind.is_disk_store() && config_load_cache_at_start {
            tracing::warn!(
                cache_kind = cache_kind.name(),
                "config_load_cache_at_start is set but cache is memory-only, \
                 nothing will be loaded at startup"
            );
        }
        // Create unified cache using the Cache framework with CacheData directly
        let cache_server_name = client_props.get_cache_server_name();
        let unified_cache: Cache<CacheData> = CacheBuilder::config(cache_ns)
            .load_cache_at_start(config_load_cache_at_start)
            .store(cache_kind, client_props.get_cache_dir(), &cache_server_name)
            .build()
            .await;
        let unified_cache = Arc::new(unified_cache);
        if config_load_cache_at_start {
            Self::drop_empty_entries(&unified_cache);
        }
        let config_filters = Arc::new(config_filters);

        // group_key: String
        let (notify_change_tx, notify_change_rx) = tokio::sync::mpsc::channel(16);
        let notify_change_tx_clone = notify_change_tx.clone();

        let server_list_provider = create_server_list_provider(&client_props).await?;

        let remote_client = NacosGrpcClientBuilder::new(server_list_provider)
            .port(client_props.get_remote_grpc_port())
            .namespace(client_props.get_namespace())
            .app_name(client_props.get_app_name())
            .client_version(client_props.get_client_version())
            .add_default_labels(crate::api::constants::common_remote::LABEL_MODULE_CONFIG)
            .add_labels(client_props.get_labels())
            .register_server_request_handler::<ConfigChangeNotifyRequest>(Arc::new(
                ConfigChangeNotifyHandler { notify_change_tx },
            ))
            .auth_plugin(auth_plugin)
            .auth_context(client_props.get_auth_context())
            .emergency_start(client_props.get_config_load_cache_at_start())
            .build(client_id)
            .await?;

        let remote_client = Arc::new(remote_client);
        // todo Event/Subscriber instead of mpsc Sender/Receiver
        crate::common::executor::spawn(Self::notify_change_to_cache_data(
            Arc::clone(&remote_client),
            Arc::clone(&unified_cache),
            notify_change_rx,
            Arc::clone(&config_filters),
        ));

        crate::common::executor::spawn(Self::list_ensure_cache_data_newest(
            Arc::clone(&remote_client),
            Arc::clone(&unified_cache),
            notify_change_tx_clone,
        ));

        Ok(Self {
            namespace,
            remote_client,
            unified_cache,
            config_filters,
        })
    }
}

impl ConfigWorker {
    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }

    #[instrument(skip_all)]
    pub(crate) async fn get_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<ConfigResponse> {
        let group_key = util::group_key(&data_id, &group, &self.namespace);

        // Try to get config from cache first (if cache exists and content is complete)
        let cached_source = self.unified_cache.get(&group_key).and_then(|r| {
            if !r.content.is_empty() && !r.md5.is_empty() {
                Some(r.resp_source(&self.config_filters))
            } else {
                None
            }
        });
        if let Some(source) = cached_source {
            tracing::info!("get_config from cache, group_key={}", group_key);
            return Ok(source.into_response().await);
        }

        // Cache miss or incomplete content, fetch from server
        let config_resp = Self::get_config_inner_async(
            self.remote_client.clone(),
            data_id.clone(),
            group.clone(),
            self.namespace.clone(),
        )
        .await?;

        let content = config_resp.content.ok_or_else(|| {
            crate::api::error::Error::ErrResult("config content missing in response".to_string())
        })?;
        let content_type = config_resp.content_type.ok_or_else(|| {
            crate::api::error::Error::ErrResult(
                "config content_type missing in response".to_string(),
            )
        })?;
        let md5 = config_resp.md5.ok_or_else(|| {
            crate::api::error::Error::ErrResult("config md5 missing in response".to_string())
        })?;

        // invoke config_filter
        let mut conf_resp = ConfigResp::new(
            data_id,
            group,
            self.namespace.clone(),
            content,
            config_resp.encrypted_data_key.unwrap_or_default(),
        );
        for config_filter in self.config_filters.iter() {
            config_filter.filter(None, Some(&mut conf_resp)).await;
        }

        Ok(ConfigResponse::new(
            conf_resp.data_id,
            conf_resp.group,
            conf_resp.namespace,
            conf_resp.content,
            content_type,
            md5,
        ))
    }

    #[instrument(skip_all)]
    pub(crate) async fn publish_config(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
    ) -> crate::api::error::Result<bool> {
        self.publish_config_inner(data_id, group, content, content_type, None, None, None)
            .await
    }

    #[instrument(skip_all)]
    pub(crate) async fn publish_config_cas(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: String,
    ) -> crate::api::error::Result<bool> {
        self.publish_config_inner(
            data_id,
            group,
            content,
            content_type,
            Some(cas_md5),
            None,
            None,
        )
        .await
    }

    #[instrument(skip_all)]
    pub(crate) async fn publish_config_beta(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        beta_ips: String,
    ) -> crate::api::error::Result<bool> {
        self.publish_config_inner(
            data_id,
            group,
            content,
            content_type,
            None,
            Some(beta_ips),
            None,
        )
        .await
    }

    #[instrument(skip_all)]
    pub(crate) async fn publish_config_param(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        params: HashMap<String, String>,
    ) -> crate::api::error::Result<bool> {
        self.publish_config_inner(
            data_id,
            group,
            content,
            content_type,
            cas_md5,
            None,
            Some(params),
        )
        .await
    }

    /// Internal implementation for all publish_config variants.
    #[allow(clippy::too_many_arguments)]
    async fn publish_config_inner(
        &self,
        data_id: String,
        group: String,
        content: String,
        content_type: Option<String>,
        cas_md5: Option<String>,
        beta_ips: Option<String>,
        params: Option<HashMap<String, String>>,
    ) -> crate::api::error::Result<bool> {
        let mut conf_req = ConfigReq::new(
            data_id,
            group,
            self.namespace.clone(),
            content,
            "".to_string(),
        );
        for config_filter in self.config_filters.iter() {
            config_filter.filter(Some(&mut conf_req), None).await;
        }

        Self::publish_config_inner_async(
            self.remote_client.clone(),
            conf_req.data_id,
            conf_req.group,
            conf_req.namespace,
            conf_req.content,
            content_type,
            conf_req.encrypted_data_key,
            cas_md5,
            beta_ips,
            params,
        )
        .await
    }

    #[instrument(skip_all)]
    pub(crate) async fn remove_config(
        &self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<bool> {
        Self::remove_config_inner_async(
            self.remote_client.clone(),
            data_id,
            group,
            self.namespace.clone(),
        )
        .await
    }

    /// Add listener.
    #[instrument(skip_all)]
    pub(crate) async fn add_listener(
        &self,
        data_id: String,
        group: String,
        listener: Arc<dyn crate::api::config::ConfigChangeListener>,
    ) {
        let group_key = util::group_key(&data_id, &group, &self.namespace);

        if !self.unified_cache.contains_key(&group_key) {
            let mut cache_data = CacheData::new(data_id, group, self.namespace.clone());

            // listen immediately upon initialization
            let config_resp = Self::get_config_inner_async(
                self.remote_client.clone(),
                cache_data.data_id.to_string(),
                cache_data.group.to_string(),
                cache_data.namespace.to_string(),
            )
            .in_current_span()
            .await;
            match config_resp {
                Ok(config_resp) => {
                    Self::fill_data_and_notify(&mut cache_data, config_resp, &self.config_filters)
                        .await;
                }
                Err(e) => {
                    tracing::error!("get_config_inner_async, config_resp err={e:?}");
                }
            }
            let req = ConfigBatchListenRequest::new(true).add_config_listen_context(
                ConfigListenContext::new(
                    Arc::clone(&cache_data.data_id),
                    Arc::clone(&cache_data.group),
                    Arc::clone(&cache_data.namespace),
                    Arc::clone(&cache_data.md5),
                ),
            );
            let remote_client_clone = self.remote_client.clone();
            crate::common::executor::spawn(
                async move {
                    let _ = remote_client_clone
                        .send_request::<ConfigBatchListenRequest, ConfigChangeBatchListenResponse>(
                            req,
                        )
                        .await;
                }
                .in_current_span(),
            );
            // Save to unified cache (will persist to disk)
            self.unified_cache.insert(group_key.clone(), cache_data);
        }
        // Add listener to existing cache data
        if let Some(cache_ref) = self.unified_cache.get(&group_key) {
            cache_ref.add_listener(listener);
        }
    }

    /// Remove listener.
    #[instrument(skip_all)]
    pub(crate) async fn remove_listener(
        &self,
        data_id: String,
        group: String,
        listener: Arc<dyn crate::api::config::ConfigChangeListener>,
    ) {
        let group_key = util::group_key(&data_id, &group, &self.namespace);

        if let Some(cache_ref) = self.unified_cache.get(&group_key) {
            cache_ref.remove_listener(listener);
        }
    }
}

impl ConfigWorker {
    /// List-Watch, list ensure cache-data newest.
    #[instrument(skip_all)]
    async fn list_ensure_cache_data_newest(
        remote_client: Arc<NacosGrpcClient>,
        unified_cache: Arc<Cache<CacheData>>,
        notify_change_tx: tokio::sync::mpsc::Sender<String>,
    ) {
        tracing::info!("list_ensure_cache_data_newest started");
        loop {
            tracing::debug!("list_ensure_cache_data_newest refreshing");
            // todo invoke remove_listener with ConfigBatchListenClientRequest::new(false) when is_empty(),
            //  and then remove cache_data from unified_cache.
            let mut listen_context_vec = Vec::with_capacity(6);
            {
                // Use foreach to iterate over all cached configs
                unified_cache.for_each(|_key, c| {
                    listen_context_vec.push(ConfigListenContext::new(
                        Arc::clone(&c.data_id),
                        Arc::clone(&c.group),
                        Arc::clone(&c.namespace),
                        Arc::clone(&c.md5),
                    ));
                });
            }
            if !listen_context_vec.is_empty() {
                tracing::debug!("list_ensure_cache_data_newest context={listen_context_vec:?}");
                let req =
                    ConfigBatchListenRequest::new(true).config_listen_context(listen_context_vec);

                let resp = remote_client
                    .send_request::<ConfigBatchListenRequest, ConfigChangeBatchListenResponse>(req)
                    .in_current_span()
                    .await;

                if let Ok(resp) = resp
                    && resp.is_success()
                    && let Some(change_context_vec) = resp.changed_configs
                {
                    for context in change_context_vec {
                        // notify config change
                        let group_key =
                            util::group_key(&context.data_id, &context.group, &context.namespace);
                        let _ = notify_change_tx.send(group_key).await;
                    }
                }
            }
            tracing::debug!("list_ensure_cache_data_newest finish");

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }

    /// Synchronously fill fields from the response; returns whether listeners should
    /// be notified (the first fill during initialization does not notify).
    ///
    /// A rejected response consumes the initialization window without touching the
    /// cached fields, so the next successful fill still notifies listeners.
    fn fill_data(
        cache_data: &mut CacheData,
        config_resp: ConfigQueryResponse,
    ) -> crate::api::error::Result<bool> {
        let first_fill = std::mem::take(&mut cache_data.initializing);
        let content_type = config_resp.content_type.ok_or_else(|| {
            crate::api::error::Error::ErrResult(
                "config content_type missing in response".to_string(),
            )
        })?;
        let content = config_resp.content.ok_or_else(|| {
            crate::api::error::Error::ErrResult("config content missing in response".to_string())
        })?;
        let md5 = config_resp.md5.ok_or_else(|| {
            crate::api::error::Error::ErrResult("config md5 missing in response".to_string())
        })?;
        cache_data.content_type = Arc::new(content_type);
        cache_data.content = Arc::new(content);
        cache_data.md5 = Arc::new(md5);
        // Compatibility None < 2.1.0
        cache_data.encrypted_data_key =
            Arc::new(config_resp.encrypted_data_key.unwrap_or_default());
        cache_data.last_modified = config_resp.last_modified;
        tracing::info!("fill_data, cache_data={}", cache_data);
        Ok(!first_fill)
    }

    async fn fill_data_and_notify(
        cache_data: &mut CacheData,
        config_resp: ConfigQueryResponse,
        config_filters: &Arc<Vec<Box<dyn ConfigFilter>>>,
    ) {
        match Self::fill_data(cache_data, config_resp) {
            Ok(true) => {
                // check md5 and then notify
                cache_data.notify_listener(config_filters).await;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(
                data_id = %cache_data.data_id,
                group = %cache_data.group,
                namespace = %cache_data.namespace,
                "fill cache data failed, skip notify: {e:?}"
            ),
        }
    }

    /// Dispatch the cached revision without holding a DashMap guard across the
    /// async config filters.
    async fn dispatch_cached(
        unified_cache: &Cache<CacheData>,
        group_key: &str,
        config_filters: &Arc<Vec<Box<dyn ConfigFilter>>>,
    ) {
        let Some(source) = unified_cache
            .get(group_key)
            .map(|r| r.resp_source(config_filters))
        else {
            return;
        };
        let resp = source.into_response().await;
        if let Some(cache_ref) = unified_cache.get(group_key) {
            cache_ref.dispatch_notify(resp);
        }
    }

    /// A missing config is reported by the server with an empty md5. Converge the
    /// local entry to the same state; keeping the stale md5 makes every list-watch
    /// round flag the key as changed and re-fetch it forever.
    ///
    /// Returns `None` when there was nothing to clear, otherwise whether listeners
    /// should be notified.
    fn clear_deleted(cache_data: &mut CacheData, group_key: &str) -> Option<bool> {
        let first_fill = std::mem::take(&mut cache_data.initializing);
        if cache_data.content.is_empty() && cache_data.md5.is_empty() {
            return None;
        }
        cache_data.content = Arc::new(String::new());
        cache_data.md5 = Arc::new(String::new());
        cache_data.encrypted_data_key = Arc::new(String::new());
        tracing::warn!(
            group_key = %group_key,
            "config no longer exists on server, cleared the stale cache entry"
        );
        Some(!first_fill)
    }

    /// Keep the entry (and its listeners) so a re-created config is still
    /// observed, but drop the deleted content and notify the change. The snapshot
    /// file is deleted like the Java client's `saveSnapshot(null)`, so the next
    /// startup does not load the deleted config back.
    async fn handle_config_not_found(
        unified_cache: &Cache<CacheData>,
        group_key: &str,
        config_filters: &Arc<Vec<Box<dyn ConfigFilter>>>,
    ) {
        let Some(should_notify) = unified_cache
            .get_mut(group_key)
            .and_then(|mut cache_ref_mut| Self::clear_deleted(&mut cache_ref_mut, group_key))
        else {
            return;
        };
        unified_cache.remove_persisted(group_key).await;
        if should_notify {
            Self::dispatch_cached(unified_cache, group_key, config_filters).await;
        }
    }

    /// Drop entries restored from disk that only hold a deletion tombstone.
    fn drop_empty_entries(unified_cache: &Cache<CacheData>) {
        let mut empty_keys = Vec::new();
        unified_cache.for_each(|key, data| {
            if data.content.is_empty() && data.md5.is_empty() {
                empty_keys.push(key.clone());
            }
        });
        if empty_keys.is_empty() {
            return;
        }
        tracing::info!(
            count = empty_keys.len(),
            "removing empty config cache entries left by deleted configs"
        );
        for key in empty_keys {
            unified_cache.remove(&key);
        }
    }

    /// Fill a cached entry from a server response and persist the new revision.
    fn fill_cache_entry(
        unified_cache: &Cache<CacheData>,
        group_key: &str,
        config_resp: ConfigQueryResponse,
    ) -> bool {
        let Some(mut cache_ref_mut) = unified_cache.get_mut(group_key) else {
            return false;
        };
        match Self::fill_data(&mut cache_ref_mut, config_resp) {
            Ok(should_notify) => {
                cache_ref_mut.persist();
                should_notify
            }
            Err(e) => {
                tracing::warn!(
                    group_key = %group_key,
                    "fill cache data failed, skip notify: {e:?}"
                );
                false
            }
        }
    }

    /// Notify change to cache_data.
    #[instrument(skip_all)]
    async fn notify_change_to_cache_data(
        remote_client: Arc<NacosGrpcClient>,
        unified_cache: Arc<Cache<CacheData>>,
        mut notify_change_rx: tokio::sync::mpsc::Receiver<String>,
        config_filters: Arc<Vec<Box<dyn ConfigFilter>>>,
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
                    // A DashMap guard must never live across an .await: the suspended
                    // future would hold the shard lock indefinitely and freeze the
                    // single-worker SDK runtime.
                    let snapshot = unified_cache.get(&group_key).map(|r| {
                        (
                            r.data_id.to_string(),
                            r.group.to_string(),
                            r.namespace.to_string(),
                        )
                    });
                    let Some((data_id, group, namespace)) = snapshot else {
                        continue;
                    };

                    let config_resp = Self::get_config_inner_async(
                        remote_client.clone(),
                        data_id,
                        group,
                        namespace,
                    )
                    .in_current_span()
                    .await;

                    match config_resp {
                        Ok(config_resp) => {
                            let should_notify =
                                Self::fill_cache_entry(&unified_cache, &group_key, config_resp);
                            if should_notify {
                                Self::dispatch_cached(&unified_cache, &group_key, &config_filters)
                                    .await;
                            }
                        }
                        Err(crate::api::error::Error::ConfigNotFound(_)) => {
                            Self::handle_config_not_found(
                                &unified_cache,
                                &group_key,
                                &config_filters,
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!(
                                group_key = %group_key,
                                "get_config_inner_async, config_resp err={e:?}"
                            );
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
        data_id: String,
        group: String,
        namespace: String,
    ) -> crate::api::error::Result<ConfigQueryResponse> {
        let req = ConfigQueryRequest::new(data_id, group, namespace);
        let resp = remote_client
            .send_request::<ConfigQueryRequest, ConfigQueryResponse>(req)
            .await?;

        if resp.is_success() {
            return Ok(resp);
        }

        let err_str = crate::common::error::to_err_msg(&resp, "get_config");
        if resp.is_not_found() {
            Err(crate::api::error::Error::ConfigNotFound(err_str))
        } else if resp.is_query_conflict() {
            Err(crate::api::error::Error::ConfigQueryConflict(err_str))
        } else {
            Err(crate::api::error::Error::ErrResult(err_str))
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn publish_config_inner_async(
        remote_client: Arc<NacosGrpcClient>,
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
            .send_request::<ConfigPublishRequest, ConfigPublishResponse>(req)
            .await?;

        crate::common::error::handle_response(&resp, "publish_config").map(|_| true)
    }

    async fn remove_config_inner_async(
        remote_client: Arc<NacosGrpcClient>,
        data_id: String,
        group: String,
        namespace: String,
    ) -> crate::api::error::Result<bool> {
        let req = ConfigRemoveRequest::new(data_id, group, namespace);
        let resp = remote_client
            .send_request::<ConfigRemoveRequest, ConfigRemoveResponse>(req)
            .await?;

        crate::common::error::handle_response(&resp, "remove_config").map(|_| true)
    }
}

#[cfg(test)]
mod tests;

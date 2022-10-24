use crate::api::config::ConfigResponse;
use crate::api::plugin::{ConfigFilter, ConfigReq, ConfigResp};
use crate::api::props::ClientProps;
use crate::common::remote::conn::Connection;
use crate::common::remote::request::client_request::*;
use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::client_response::*;
use crate::common::remote::response::server_response::*;
use crate::common::remote::response::*;
use crate::common::util::payload_helper;
use crate::common::util::payload_helper::PayloadInner;
use crate::config::cache::CacheData;
use crate::config::client_request::*;
use crate::config::client_response::*;
use crate::config::server_request::*;
use crate::config::server_response::*;
use crate::config::util;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct ConfigWorker {
    client_props: ClientProps,
    connection: Connection,
    cache_data_map: Arc<Mutex<HashMap<String, CacheData>>>,
    config_filters: Arc<Vec<Box<dyn ConfigFilter>>>,
}

impl ConfigWorker {
    pub(crate) fn new(
        client_props: ClientProps,
        config_filters: Vec<Box<dyn ConfigFilter>>,
    ) -> Self {
        let connection = Connection::new(client_props.clone());
        let cache_data_map = Arc::new(Mutex::new(HashMap::new()));
        let config_filters = Arc::new(config_filters);

        Self {
            client_props,
            connection,
            cache_data_map,
            config_filters,
        }
    }

    /// start Once
    pub(crate) async fn start(&mut self) {
        self.connection.connect().await;

        let mut conn = self.connection.clone();
        // group_key: String
        let (notify_change_tx, notify_change_rx) = tokio::sync::mpsc::channel(16);
        let notify_change_tx_1 = notify_change_tx.clone();
        let notify_change_tx_2 = notify_change_tx.clone();

        let _conn_thread = std::thread::Builder::new()
            .name("config-remote-client".into())
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                    .expect("config-remote-client runtime initialization failed");

                runtime.block_on(async move {
                    let (server_req_payload_tx, mut server_req_payload_rx) = tokio::sync::mpsc::channel(128);
                    loop {
                        tokio::select! { biased;
                            // deal with next_server_req_payload, basic conn interaction logic.
                            server_req_payload_inner = conn.next_server_req_payload() => {
                                let payload_inner = server_req_payload_inner;
                                if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&payload_inner.type_url) {
                                    tracing::debug!(
                                        "[{}] receive client-detection, {} with {}",
                                        conn.get_conn_id(), payload_inner.type_url, payload_inner.body_str
                                    );
                                    let de = ClientDetectionServerRequest::from(payload_inner.body_str.as_str()).headers(payload_inner.headers);
                                    let _ = conn.reply_client_resp(ClientDetectionClientResponse::new(de.request_id().clone())).await;
                                } else if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&payload_inner.type_url) {
                                    tracing::warn!(
                                        "[{}] receive connect-reset, {} with {}",
                                        conn.get_conn_id(), payload_inner.type_url, payload_inner.body_str
                                    );
                                    let de = ConnectResetServerRequest::from(payload_inner.body_str.as_str()).headers(payload_inner.headers);
                                    let _ = conn.reply_client_resp(ConnectResetClientResponse::new(de.request_id().clone())).await;
                                    // todo reset connection
                                } else {
                                    tracing::info!(
                                        "[{}] receive server-request, {} with {}",
                                        conn.get_conn_id(), payload_inner.type_url, payload_inner.body_str
                                    );
                                    // publish a server_req_payload, server_req_payload_rx receive it once.
                                    if let Err(e) = server_req_payload_tx.send(payload_inner).await {
                                        tracing::error!("receiver dropped by {:?}", e)
                                    }
                                }
                            },
                            // receive a server_req from server_req_payload_tx
                            receive_server_req = server_req_payload_rx.recv() => {
                                Self::deal_extra_server_req(notify_change_tx_1.clone(), &mut conn, receive_server_req.unwrap()).await
                            },
                        }
                    }
                });
            })
            .expect("config-remote-client could not spawn thread");

        tokio::spawn(Self::notify_change_to_cache_data(
            self.connection.clone(),
            Arc::clone(&self.cache_data_map),
            notify_change_rx,
        ));
        tokio::spawn(Self::list_ensure_cache_data_newest(
            self.connection.clone(),
            Arc::clone(&self.cache_data_map),
            notify_change_tx_2,
        ));

        loop {
            // sleep 6ms, Make sure the link is established.
            tokio::time::sleep(std::time::Duration::from_millis(6)).await;

            if let Ok(client) = self.connection.get_client() {
                let resp = client.request(&payload_helper::build_req_grpc_payload(
                    HealthCheckClientRequest::new(),
                ));
                if let Ok(resp) = resp {
                    let resp = payload_helper::build_server_response(resp).unwrap();
                    if resp.is_success() {
                        break;
                    }
                }
            }
        }
    }

    pub(crate) fn get_config(
        &mut self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<ConfigResponse> {
        let namespace = self.client_props.namespace.clone();
        let config_resp = Self::get_config_inner(
            &mut self.connection,
            data_id.clone(),
            group.clone(),
            namespace.clone(),
        )?;

        // invoke config_filter
        let mut conf_resp = ConfigResp::new(
            data_id,
            group,
            namespace,
            config_resp.content().unwrap().to_string(),
            config_resp
                .encrypted_data_key()
                .unwrap_or(&"".to_string())
                .to_string(),
        );
        for config_filter in self.config_filters.iter() {
            config_filter.filter(None, Some(&mut conf_resp));
        }

        Ok(ConfigResponse::new(
            conf_resp.data_id,
            conf_resp.group,
            conf_resp.namespace,
            conf_resp.content,
            config_resp.content_type().unwrap().to_string(),
            config_resp.md5().unwrap().to_string(),
        ))
    }

    pub(crate) fn publish_config(
        &mut self,
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

        Self::publish_config_inner(
            &mut self.connection,
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
    }

    pub(crate) fn publish_config_cas(
        &mut self,
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

        Self::publish_config_inner(
            &mut self.connection,
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
    }

    pub(crate) fn publish_config_beta(
        &mut self,
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

        Self::publish_config_inner(
            &mut self.connection,
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
    }

    pub(crate) fn publish_config_param(
        &mut self,
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

        Self::publish_config_inner(
            &mut self.connection,
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
    }

    pub(crate) fn remove_config(
        &mut self,
        data_id: String,
        group: String,
    ) -> crate::api::error::Result<bool> {
        let namespace = self.client_props.namespace.clone();
        Self::remove_config_inner(&mut self.connection, data_id, group, namespace)
    }

    /// Add listener.
    pub(crate) fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: Arc<dyn crate::api::config::ConfigChangeListener>,
    ) {
        let namespace = self.client_props.namespace.clone();
        let group_key = util::group_key(&data_id, &group, &namespace);
        if let Ok(mut mutex) = self.cache_data_map.lock() {
            if !mutex.contains_key(group_key.as_str()) {
                let mut cache_data =
                    CacheData::new(self.config_filters.clone(), data_id, group, namespace);
                // listen immediately upon initialization
                if let Ok(client) = self.connection.get_client() {
                    // init cache_data
                    let config_resp = Self::get_config_inner(
                        &mut self.connection,
                        cache_data.data_id.clone(),
                        cache_data.group.clone(),
                        cache_data.namespace.clone(),
                    );
                    if let Ok(config_resp) = config_resp {
                        Self::fill_data_and_notify(&mut cache_data, config_resp);
                    }
                    let req = ConfigBatchListenClientRequest::new(true).add_config_listen_context(
                        ConfigListenContext::new(
                            cache_data.data_id.clone(),
                            cache_data.group.clone(),
                            cache_data.namespace.clone(),
                            cache_data.md5.clone(),
                        ),
                    );
                    let _ = client.request(&payload_helper::build_req_grpc_payload(req));
                }
                mutex.insert(group_key.clone(), cache_data);
            }
            let _ = mutex
                .get_mut(group_key.as_str())
                .map(|c| c.add_listener(listener));
        }
    }

    /// Remove listener.
    pub(crate) fn remove_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: Arc<dyn crate::api::config::ConfigChangeListener>,
    ) {
        let namespace = self.client_props.namespace.clone();
        let group_key = util::group_key(&data_id, &group, &namespace);
        if let Ok(mut mutex) = self.cache_data_map.lock() {
            if !mutex.contains_key(group_key.as_str()) {
                return;
            }
            let _ = mutex
                .get_mut(group_key.as_str())
                .map(|c| c.remove_listener(listener));
        }
    }
}

impl ConfigWorker {
    async fn deal_extra_server_req(
        notify_change_tx: tokio::sync::mpsc::Sender<String>,
        conn: &mut Connection,
        payload_inner: PayloadInner,
    ) {
        if TYPE_CONFIG_CHANGE_NOTIFY_SERVER_REQUEST.eq(&payload_inner.type_url) {
            let server_req = ConfigChangeNotifyServerRequest::from(payload_inner.body_str.as_str())
                .headers(payload_inner.headers);
            let server_req_id = server_req.request_id().clone();
            let req_namespace = server_req.tenant.unwrap_or_else(|| "".to_string());
            tracing::info!(
                "[{}] receive config-change, dataId={},group={},namespace={}",
                conn.get_conn_id(),
                server_req.dataId,
                server_req.group,
                req_namespace
            );
            // notify config change
            let group_key = util::group_key(&server_req.dataId, &server_req.group, &req_namespace);
            let _ = notify_change_tx.send(group_key).await;
            // reply ConfigChangeNotifyClientResponse for ConfigChangeNotifyServerRequest
            let _ = conn
                .reply_client_resp(ConfigChangeNotifyClientResponse::new(server_req_id))
                .await;
        } else {
            tracing::warn!(
                "unknown receive type_url={}, maybe sdk have to upgrade!",
                &payload_inner.type_url
            );
        }
    }

    /// List-Watch, list ensure cache-data newest.
    async fn list_ensure_cache_data_newest(
        mut connection: Connection,
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
                if let Ok(client) = connection.get_client() {
                    let req = ConfigBatchListenClientRequest::new(true)
                        .config_listen_context(listen_context_vec);
                    let resp = client.request(&payload_helper::build_req_grpc_payload(req));
                    if let Ok(resp) = resp {
                        let payload_inner = payload_helper::covert_payload(resp);
                        if TYPE_CONFIG_CHANGE_BATCH_LISTEN_RESPONSE.eq(&payload_inner.type_url) {
                            let batch_listen_resp = ConfigChangeBatchListenServerResponse::from(
                                payload_inner.body_str.as_str(),
                            );
                            if let Some(change_context_vec) = batch_listen_resp.changed_configs() {
                                for context in change_context_vec {
                                    // notify config change
                                    let group_key = util::group_key(
                                        &context.dataId,
                                        &context.group,
                                        &context.tenant,
                                    );
                                    let _ = notify_change_tx.send(group_key).await;
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    }

    /// Notify change to cache_data.
    async fn notify_change_to_cache_data(
        mut connection: Connection,
        cache_data_map: Arc<Mutex<HashMap<String, CacheData>>>,
        mut notify_change_rx: tokio::sync::mpsc::Receiver<String>,
    ) {
        loop {
            match notify_change_rx.recv().await {
                None => {
                    tracing::warn!(
                        "[{}] notify_change_to_cache_data break, notify_change_rx be dropped(shutdown).",
                        connection.get_conn_id()
                    );
                    break;
                }
                Some(group_key) => {
                    if let Ok(mut mutex) = cache_data_map.lock() {
                        if !mutex.contains_key(group_key.as_str()) {
                            continue;
                        }
                        let _ = mutex.get_mut(group_key.as_str()).map(|c| {
                            // get the newest config to notify
                            let config_resp = Self::get_config_inner(
                                &mut connection,
                                c.data_id.clone(),
                                c.group.clone(),
                                c.namespace.clone(),
                            );
                            if let Ok(config_resp) = config_resp {
                                Self::fill_data_and_notify(c, config_resp);
                            }
                        });
                    }
                }
            }
        }
    }

    fn fill_data_and_notify(cache_data: &mut CacheData, config_resp: ConfigQueryServerResponse) {
        cache_data.content_type = config_resp.content_type().unwrap().to_string();
        cache_data.content = config_resp.content().unwrap().to_string();
        cache_data.md5 = config_resp.md5().unwrap().to_string();
        // Compatibility None < 2.1.0
        cache_data.encrypted_data_key = config_resp
            .encrypted_data_key()
            .unwrap_or(&"".to_string())
            .to_string();
        cache_data.last_modified = config_resp.last_modified();
        tracing::info!("fill_data_and_notify, cache_data={}", cache_data);
        if cache_data.initializing {
            cache_data.initializing = false;
        } else {
            // check md5 and then notify
            cache_data.notify_listener();
        }
    }

    fn get_config_inner(
        connection: &mut Connection,
        data_id: String,
        group: String,
        namespace: String,
    ) -> crate::api::error::Result<ConfigQueryServerResponse> {
        let req = ConfigQueryClientRequest::new(data_id, group, namespace);
        let req_payload = payload_helper::build_req_grpc_payload(req);
        let resp = connection.get_client()?.request(&req_payload)?;
        let payload_inner = payload_helper::covert_payload(resp);
        // return Err if get a err_resp
        if payload_helper::is_err_resp(&payload_inner.type_url) {
            let err_resp = ErrorResponse::from(payload_inner.body_str.as_str());
            return Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                err_resp.error_code(),
                err_resp.message().unwrap()
            )));
        }
        let config_resp = ConfigQueryServerResponse::from(payload_inner.body_str.as_str());
        if config_resp.is_success() {
            Ok(config_resp)
        } else if config_resp.is_not_found() {
            Err(crate::api::error::Error::ConfigNotFound(format!(
                "error_code={},message={}",
                config_resp.error_code(),
                config_resp.message().unwrap()
            )))
        } else if config_resp.is_query_conflict() {
            Err(crate::api::error::Error::ConfigQueryConflict(format!(
                "error_code={},message={}",
                config_resp.error_code(),
                config_resp.message().unwrap()
            )))
        } else {
            Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                config_resp.error_code(),
                config_resp.message().unwrap()
            )))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_config_inner(
        connection: &mut Connection,
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
            ConfigPublishClientRequest::new(data_id, group, namespace, content).cas_md5(cas_md5);
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

        let req_payload = payload_helper::build_req_grpc_payload(req);
        let resp = connection.get_client()?.request(&req_payload)?;
        let payload_inner = payload_helper::covert_payload(resp);
        // return Err if get a err_resp
        if payload_helper::is_err_resp(&payload_inner.type_url) {
            let err_resp = ErrorResponse::from(payload_inner.body_str.as_str());
            return Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                err_resp.error_code(),
                err_resp.message().unwrap()
            )));
        }

        let publish_resp = ConfigPublishServerResponse::from(payload_inner.body_str.as_str());
        Ok(publish_resp.is_success())
    }

    fn remove_config_inner(
        connection: &mut Connection,
        data_id: String,
        group: String,
        namespace: String,
    ) -> crate::api::error::Result<bool> {
        let req = ConfigRemoveClientRequest::new(data_id, group, namespace);
        let req_payload = payload_helper::build_req_grpc_payload(req);
        let resp = connection.get_client()?.request(&req_payload)?;
        let payload_inner = payload_helper::covert_payload(resp);
        // return Err if get a err_resp
        if payload_helper::is_err_resp(&payload_inner.type_url) {
            let err_resp = ErrorResponse::from(payload_inner.body_str.as_str());
            return Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                err_resp.error_code(),
                err_resp.message().unwrap()
            )));
        }
        let remove_resp = ConfigRemoveServerResponse::from(payload_inner.body_str.as_str());
        Ok(remove_resp.is_success())
    }
}

#[cfg(test)]
mod tests {
    use crate::api::config::{ConfigChangeListener, ConfigResponse};
    use crate::api::props::ClientProps;
    use crate::config::util;
    use crate::config::worker::ConfigWorker;
    use std::sync::Arc;

    #[test]
    fn test_client_worker_add_listener() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let mut client_worker =
            ConfigWorker::new(ClientProps::new().namespace(n.clone()), Vec::new());

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
            let cache_data_map_mutex = client_worker.cache_data_map.lock().unwrap();
            let cache_data = cache_data_map_mutex.get(group_key.as_str()).unwrap();
            let listen_mutex = cache_data.listeners.lock().unwrap();
            assert_eq!(2, listen_mutex.len());
        }
    }

    #[test]
    fn test_client_worker_add_listener_then_remove() {
        let (d, g, n) = ("D".to_string(), "G".to_string(), "N".to_string());

        let mut client_worker =
            ConfigWorker::new(ClientProps::new().namespace(n.clone()), Vec::new());

        // test add listener1
        let lis1_arc = Arc::new(TestConfigChangeListener1 {});
        let lis1_arc2 = Arc::clone(&lis1_arc);
        let _listen = client_worker.add_listener(d.clone(), g.clone(), lis1_arc);

        let group_key = util::group_key(&d, &g, &n);
        {
            let cache_data_map_mutex = client_worker.cache_data_map.lock().unwrap();
            let cache_data = cache_data_map_mutex.get(group_key.as_str()).unwrap();
            let listen_mutex = cache_data.listeners.lock().unwrap();
            assert_eq!(1, listen_mutex.len());
        }

        client_worker.remove_listener(d.clone(), g.clone(), lis1_arc2);
        {
            let cache_data_map_mutex = client_worker.cache_data_map.lock().unwrap();
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

use crate::api::config::ConfigResponse;
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
}

impl ConfigWorker {
    pub(crate) fn new(client_props: ClientProps) -> Self {
        let connection = Connection::new(client_props.clone());
        let cache_data_map = Arc::new(Mutex::new(HashMap::new()));

        Self {
            client_props,
            connection,
            cache_data_map,
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
                                    if let Err(_) = server_req_payload_tx.send(payload_inner).await {
                                        tracing::error!("receiver dropped")
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
        let tenant = self.client_props.namespace.clone();
        let config_resp = Self::get_config_inner(
            &mut self.connection,
            data_id.clone(),
            group.clone(),
            tenant.clone(),
        )?;
        Ok(ConfigResponse::new(
            data_id,
            group,
            tenant,
            config_resp.content().unwrap().to_string(),
            config_resp.content_type().unwrap().to_string(),
        ))
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
        if let Ok(mut mutex) = self.cache_data_map.lock() {
            if !mutex.contains_key(group_key.as_str()) {
                let mut cache_data = CacheData::new(data_id, group, tenant);
                // listen immediately upon initialization
                if let Ok(client) = self.connection.get_client() {
                    // init cache_data
                    let config_resp = Self::get_config_inner(
                        &mut self.connection,
                        cache_data.data_id.clone(),
                        cache_data.group.clone(),
                        cache_data.tenant.clone(),
                    );
                    if let Ok(config_resp) = config_resp {
                        Self::fill_data_and_notify(&mut cache_data, config_resp);
                    }
                    let req = ConfigBatchListenClientRequest::new(true).add_config_listen_context(
                        ConfigListenContext::new(
                            cache_data.data_id.clone(),
                            cache_data.group.clone(),
                            cache_data.tenant.clone(),
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
            let req_tenant = server_req.tenant.unwrap_or_else(|| "".to_string());
            tracing::info!(
                "[{}] receive config-change, dataId={},group={},namespace={}",
                conn.get_conn_id(),
                server_req.dataId,
                server_req.group,
                req_tenant
            );
            // notify config change
            let group_key = util::group_key(&server_req.dataId, &server_req.group, &req_tenant);
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
            let mut listen_context_vec = Vec::with_capacity(6);
            {
                // try_lock, The failure to acquire the lock can be handled by the next loop.
                if let Ok(mutex) = cache_data_map.try_lock() {
                    for c in mutex.values() {
                        listen_context_vec.push(ConfigListenContext::new(
                            c.data_id.clone(),
                            c.group.clone(),
                            c.tenant.clone(),
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
                                c.tenant.clone(),
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
        tenant: String,
    ) -> crate::api::error::Result<ConfigQueryServerResponse> {
        let req = ConfigQueryClientRequest::new(data_id, group, tenant);
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
        } else {
            Err(crate::api::error::Error::ErrResult(format!(
                "error_code={},message={}",
                config_resp.error_code(),
                config_resp.message().unwrap()
            )))
        }
    }
}

/// Cache Data for Config
#[derive(Default)]
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

    /// who listen of config change.
    listeners: Arc<Mutex<Vec<ListenerWrapper>>>,
}

impl CacheData {
    fn new(data_id: String, group: String, tenant: String) -> Self {
        Self {
            data_id,
            group,
            tenant,
            content_type: "text".to_string(),
            initializing: true,
            ..Default::default()
        }
    }

    /// Add listener.
    fn add_listener(&mut self, listener: Box<crate::api::config::ConfigChangeListener>) {
        if let Ok(mut mutex) = self.listeners.lock() {
            mutex.push(ListenerWrapper::new(listener));
        }
    }

    /// Notify listener. when last-md5 not equals the-newest-md5
    fn notify_listener(&mut self) {
        if let Ok(mut mutex) = self.listeners.lock() {
            for listen in mutex.iter_mut() {
                if listen.last_md5.eq(&self.md5) {
                    continue;
                }
                // Notify when last-md5 not equals the-newest-md5, todo Notify in independent thread.
                (listen.listener)(ConfigResponse::new(
                    self.data_id.clone(),
                    self.group.clone(),
                    self.tenant.clone(),
                    self.content.clone(),
                    self.content_type.clone(),
                ));
                listen.last_md5 = self.md5.clone();
                tracing::info!(
                    "notify_listener success, dataId={},group={},namespace={},md5={}",
                    self.data_id,
                    self.group,
                    self.tenant,
                    self.md5
                );
            }
        }
    }
}

impl std::fmt::Display for CacheData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.content.len() > 30 {
            let mut content = self.content.clone();
            content.truncate(30);
            content.push_str("...");
            write!(
                f,
                "CacheData(namespace={n},data_id={d},group={g},md5={m},encrypted_data_key={k},content_type={t},content={c})",
                n = self.tenant,
                d = self.data_id,
                g = self.group,
                m = self.md5,
                k = self.encrypted_data_key.as_ref().unwrap_or(&"".to_string()),
                t = self.content_type,
                c = content
            )
        } else {
            write!(
                f,
                "CacheData(namespace={n},data_id={d},group={g},md5={m},encrypted_data_key={k},content_type={t},content={c})",
                n = self.tenant,
                d = self.data_id,
                g = self.group,
                m = self.md5,
                k = self.encrypted_data_key.as_ref().unwrap_or(&"".to_string()),
                t = self.content_type,
                c = self.content
            )
        }
    }
}

/// The inner Wrapper of ConfigChangeListener
struct ListenerWrapper {
    /// last md5 be notified
    last_md5: String,
    listener: Box<crate::api::config::ConfigChangeListener>,
}

impl ListenerWrapper {
    fn new(listener: Box<crate::api::config::ConfigChangeListener>) -> Self {
        Self {
            last_md5: "".to_string(),
            listener,
        }
    }
}

mod client_request;
mod client_response;
mod server_request;
mod server_response;
mod util;
mod worker;

use crate::api::client_config::ClientConfig;
use crate::api::config::ConfigService;
use crate::common::remote::conn::Connection;
use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::client_response::*;
use crate::common::util::payload_helper;
use crate::common::util::payload_helper::PayloadInner;
use crate::config::client_request::*;
use crate::config::client_response::*;
use crate::config::server_request::*;
use crate::config::server_response::*;
use crate::config::worker::ConfigWorker;

pub(crate) struct NacosConfigService {
    client_config: ClientConfig,
    connection: Connection,
    /// config client worker
    client_worker: ConfigWorker,
}

impl NacosConfigService {
    pub fn new(client_config: ClientConfig) -> Self {
        let connection = Connection::new(client_config.clone());
        let client_worker = ConfigWorker::new(client_config.clone());
        Self {
            client_config,
            connection,
            client_worker,
        }
    }

    /// start Once
    pub(crate) async fn start(&mut self) {
        self.connection.connect().await;

        let mut conn = self.connection.clone();
        let mut client_worker = self.client_worker.clone();

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
                            server_req_payload = conn.next_server_req_payload() => {
                                let payload_inner = payload_helper::covert_payload(server_req_payload);
                                if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&payload_inner.type_url) {
                                    let de = ClientDetectionServerRequest::from(payload_inner.body_str.as_str()).headers(payload_inner.headers);
                                    conn.reply_client_resp(ClientDetectionClientResponse::new(de.get_request_id().clone())).await;
                                } else if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&payload_inner.type_url) {
                                    let de = ConnectResetServerRequest::from(payload_inner.body_str.as_str()).headers(payload_inner.headers);
                                    conn.reply_client_resp(ConnectResetClientResponse::new(de.get_request_id().clone())).await;
                                    // todo reset connection
                                } else {
                                    // publish a server_req_payload, server_req_payload_rx receive it once.
                                    if let Err(_) = server_req_payload_tx.send(payload_inner).await {
                                        tracing::error!("receiver dropped")
                                    }
                                }
                            },
                            // receive a server_req from server_req_payload_tx
                            receive_server_req = server_req_payload_rx.recv() => {
                                Self::deal_extra_server_req(&mut client_worker, &mut conn, receive_server_req.unwrap()).await
                            },
                        }
                    }
                });
            })
            .expect("config-remote-client could not spawn thread");

        // sleep 6ms, Make sure the link is established.
        tokio::time::sleep(std::time::Duration::from_millis(6)).await
    }

    async fn deal_extra_server_req(
        client_worker: &mut ConfigWorker,
        conn: &mut Connection,
        payload_inner: PayloadInner,
    ) {
        if TYPE_CONFIG_CHANGE_NOTIFY_SERVER_REQUEST.eq(&payload_inner.type_url) {
            let server_req = ConfigChangeNotifyServerRequest::from(payload_inner.body_str.as_str())
                .headers(payload_inner.headers);
            let server_req_id = server_req.get_request_id().clone();
            let req_tenant = server_req.tenant.or(Some("".to_string())).unwrap();
            tracing::info!(
                "receiver config change, dataId={},group={},namespace={}",
                &server_req.dataId,
                &server_req.group,
                req_tenant.clone()
            );
            // notify config change
            client_worker.notify_config_change(
                server_req.dataId.to_string(),
                server_req.group.to_string(),
                req_tenant.clone(),
            );
            // reply ConfigChangeNotifyClientResponse for ConfigChangeNotifyServerRequest
            conn.reply_client_resp(ConfigChangeNotifyClientResponse::new(server_req_id))
                .await;
        } else {
            tracing::warn!(
                "unknown receive type_url={}, maybe sdk have to upgrade!",
                &payload_inner.type_url
            );
        }
    }
}

impl ConfigService for NacosConfigService {
    fn get_config(
        &mut self,
        data_id: String,
        group: String,
        _timeout_ms: u64,
    ) -> crate::api::error::Result<String> {
        let tenant = self.client_config.namespace.clone();
        let req = ConfigQueryClientRequest::new(data_id, group, tenant);
        let req_payload = payload_helper::build_req_grpc_payload(req);
        let resp = self.connection.get_client()?.request(&req_payload)?;
        let payload_inner = payload_helper::covert_payload(resp);
        let config_resp = ConfigQueryServerResponse::from(payload_inner.body_str.as_str());
        Ok(String::from(config_resp.get_content()))
    }

    fn add_listener(
        &mut self,
        data_id: String,
        group: String,
        listener: Box<crate::api::config::ConfigChangeListener>,
    ) -> crate::api::error::Result<()> {
        self.client_worker.add_listener(
            data_id.clone(),
            group.clone(),
            self.client_config.namespace.clone(),
            listener,
        );
        // todo 抽离到统一的发起地方，并取得结果
        let req = ConfigBatchListenClientRequest::new(true).add_config_listen_context(
            ConfigListenContext::new(
                data_id.clone(),
                group.clone(),
                self.client_config.namespace.clone(),
                String::from(""),
            ),
        );
        let req_payload = payload_helper::build_req_grpc_payload(req);
        let _resp_payload = self.connection.get_client()?.request(&req_payload)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::api::client_config::ClientConfig;
    use crate::api::config::ConfigService;
    use crate::config::NacosConfigService;
    use std::time::Duration;
    use tokio::time::sleep;

    // #[tokio::test]
    async fn test_config_service() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let mut config_service = NacosConfigService::new(
            ClientConfig::new()
                .server_addr("0.0.0.0:9848".to_string())
                .app_name("test-app-name"),
        );
        config_service.start().await;
        let config =
            config_service.get_config("hongwen.properties".to_string(), "LOVE".to_string(), 3000);
        match config {
            Ok(config) => tracing::info!("get the config {}", config),
            Err(err) => tracing::error!("get the config {:?}", err),
        }

        let _listen = config_service.add_listener(
            "hongwen.properties".to_string(),
            "LOVE".to_string(),
            Box::new(|config_resp| {
                tracing::info!("listen the config {}", config_resp.get_content());
            }),
        );
        match _listen {
            Ok(_) => tracing::info!("listening the config"),
            Err(err) => tracing::error!("listen config error {:?}", err),
        }

        sleep(Duration::from_secs(30)).await;
    }
}

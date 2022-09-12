mod client_request;
mod client_response;
mod server_request;
mod server_response;
mod util;

use crate::api::client_config::ClientConfig;
use crate::api::config::{ConfigResponse, ConfigService};
use crate::common::remote::conn::Connection;
use crate::common::remote::request::server_request::*;
use crate::common::remote::request::*;
use crate::common::remote::response::client_response::*;
use crate::common::util::payload_helper;
use crate::config::client_request::*;
use crate::config::client_response::*;
use crate::config::server_request::*;
use crate::config::server_response::*;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct NacosConfigService {
    client_config: ClientConfig,
    client: Option<crate::nacos_proto::v2::RequestClient>,
    conn_thread: Option<std::thread::JoinHandle<()>>,

    /// config listen context
    config_listen_context: HashMap<String, Vec<Arc<crate::api::config::ListenFn>>>,
}

impl NacosConfigService {
    pub fn new(client_config: ClientConfig) -> Self {
        Self {
            client_config,
            client: None,
            conn_thread: None,

            config_listen_context: HashMap::new(),
        }
    }

    /// start Once
    pub(crate) async fn start(&mut self) {
        let mut connection = Connection::new(self.client_config.clone());
        connection.connect().await;
        let client = connection.get_client();
        if client.is_ok() {
            self.client = Some(client.unwrap());
        }

        let conn_thread = std::thread::Builder::new()
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
                            server_req_payload = connection.next_server_req_payload() => {
                                let (type_url, headers, body_json_str) = payload_helper::covert_payload(server_req_payload);
                                if TYPE_CLIENT_DETECTION_SERVER_REQUEST.eq(&type_url) {
                                    let de = ClientDetectionServerRequest::from(body_json_str.as_str()).headers(headers);
                                    connection
                                        .reply_client_resp(ClientDetectionClientResponse::new(de.get_request_id().clone()))
                                        .await;
                                } else if TYPE_CONNECT_RESET_SERVER_REQUEST.eq(&type_url) {
                                    let de = ConnectResetServerRequest::from(body_json_str.as_str()).headers(headers);
                                    connection
                                        .reply_client_resp(ConnectResetClientResponse::new(de.get_request_id().clone()))
                                        .await;
                                    // todo reset connection
                                } else {
                                    // publish a server_req_payload, server_req_payload_rx receive it once.
                                    if let Err(_) = server_req_payload_tx.send((type_url, headers, body_json_str)).await {
                                        tracing::error!("receiver dropped")
                                    }
                                }
                            },
                            // receive a server_req from server_req_payload_tx
                            receive_server_req = server_req_payload_rx.recv() => {
                                let (type_url, headers, body_str) = receive_server_req.unwrap();
                                if TYPE_CONFIG_CHANGE_NOTIFY_SERVER_REQUEST.eq(&type_url) {
                                    let server_req = ConfigChangeNotifyServerRequest::from(body_str.as_str()).headers(headers);
                                    connection
                                        .reply_client_resp(ConfigChangeNotifyClientResponse::new(server_req.get_request_id().clone()))
                                        .await;
                                    let req_tenant = server_req.tenant.or(Some("".to_string())).unwrap();
                                    tracing::info!(
                                        "receiver config change, dataId={},group={},namespace={}",
                                        &server_req.dataId,
                                        &server_req.group,
                                        req_tenant.clone()
                                    );
                                    println!(
                                        "receiver config change, dataId={},group={},namespace={}",
                                        &server_req.dataId, &server_req.group, req_tenant.clone()
                                    );
                                    // todo notify config change
                                } else {
                                    tracing::warn!("unknown receive type_url={}, maybe sdk have to upgrade!", type_url);
                                }
                            },
                        }
                    }
                });
            })
            .expect("config-remote-client could not spawn thread");
        self.conn_thread = Some(conn_thread);

        // sleep 100ms, Make sure the link is established.
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

impl ConfigService for NacosConfigService {
    fn get_config(
        &self,
        data_id: String,
        group: String,
        _timeout_ms: u64,
    ) -> crate::api::error::Result<String> {
        if self.client.is_some() {
            let tenant = self.client_config.namespace.clone();
            let req = payload_helper::build_req_grpc_payload(ConfigQueryClientRequest::new(
                data_id, group, tenant,
            ));
            let resp = self.client.as_ref().unwrap().request(&req);
            let (_type_url, _headers, body_str) = payload_helper::covert_payload(resp.unwrap());
            let config_resp = ConfigQueryServerResponse::from(body_str.as_str());
            Ok(String::from(config_resp.get_content()))
        } else {
            Err(crate::api::error::Error::ClientShutdown(String::from(
                "Disconnected, please try later.",
            )))
        }
    }

    fn listen(
        &mut self,
        data_id: String,
        group: String,
        func: std::sync::Arc<crate::api::config::ListenFn>,
    ) -> crate::api::error::Result<()> {
        if self.client.is_some() {
            // todo 抽离到统一的发起地方
            let req = ConfigBatchListenClientRequest::new(true).add_config_listen_context(
                ConfigListenContext::new(
                    data_id.clone(),
                    group.clone(),
                    self.client_config.namespace.clone(),
                    String::from(""),
                ),
            );
            // todo 抽离到统一的发起地方，取得结果
            let req_payload = payload_helper::build_req_grpc_payload(req);
            let _resp_payload = self
                .client
                .as_ref()
                .expect("Disconnected, please try later.")
                .request(&req_payload)
                .unwrap();

            let group_key = util::group_key(&data_id, &group, &(self.client_config.namespace));
            let vec_op = self.config_listen_context.get_mut(group_key.as_str());
            if vec_op.is_some() {
                vec_op.unwrap().push(func);
            } else {
                let mut v = Vec::new();
                v.push(func);
                self.config_listen_context.insert(group_key, v);
            }
            Ok(())
        } else {
            Err(crate::api::error::Error::ClientShutdown(String::from(
                "Disconnected, please try later.",
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::client_config::ClientConfig;
    use crate::api::config::ConfigService;
    use crate::config::NacosConfigService;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_config_service() {
        let mut config_service = NacosConfigService::new(
            ClientConfig::new()
                .server_addr("0.0.0.0:9848".to_string())
                .app_name("test-app-name"),
        );
        config_service.start().await;
        let config =
            config_service.get_config("hongwen.properties".to_string(), "LOVE".to_string(), 3000);
        println!("get the config {}", config.expect("None"));

        let _listen = config_service.listen(
            "hongwen.properties".to_string(),
            "LOVE".to_string(),
            Arc::new(|config_resp| {
                println!("listen the config {}", config_resp.get_content());
            }),
        );

        sleep(Duration::from_secs(30)).await;
    }
}
